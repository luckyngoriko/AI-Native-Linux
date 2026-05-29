# Rev.3 — Constitutional Invariants (Extension)

| Field          | Value                                                                                    |
| -------------- | ---------------------------------------------------------------------------------------- |
| Status         | `CONTRACT` (created 2026-05-29; extends Rev.2 L0 invariant catalog)                      |
| Phase tag      | S0.R3                                                                                    |
| Layer          | L0 Governance, Evidence, Safety                                                          |
| Schema package | `aios.governance.v1alpha2`                                                               |
| Consumes       | `002.AI-OS.NET--SPECREV.2/L0_Governance_Evidence_Safety/04_invariants.md` (INV-001..024) |
| Produces       | INV-025..INV-034 (the Rev.3 additions to the closed `InvariantId` enum)                  |

## 1. Purpose and inheritance

Rev.3 **inherits INV-001 through INV-024 verbatim** from the Rev.2 constitutional
catalog. None is weakened, removed, or re-stated with looser language. Per Rev.2
invariant `I1` ("the invariant catalog is a closed enum; adding an invariant is a
versioned spec change"), this document is that versioned change: it extends the
`InvariantId` enum from 24 to 34 entries.

Every new constitutional rule stated in prose across the Rev.3 sections (S16–S28)
maps to either an inherited invariant or one of the additions below. New invariants
are either **specializations** of an inherited invariant for a new plane, or
**genuinely new constitutional surface** introduced by Rev.3 scope (fleet, time,
data erasure). Each retains the Rev.2 enforcer/verifier discipline (invariant `I2`).

This document also resolves the authoring-time collision in which seven Rev.3 drafts
each provisionally numbered their new rule `INV-028`. The authoritative assignment is
the table in §2; the owning sections cite these numbers.

## 2. Enum extension

```proto
// extends Rev.2 enum InvariantId { ... = 1..24 }
enum InvariantId {
  // ... INV_001..INV_024 inherited verbatim ...
  INV_025_AI_CANNOT_AUTHOR_EBPF                 = 25;
  INV_026_CLUSTER_ROOT_CANNOT_OVERRIDE_HOST     = 26;
  INV_027_CRYPTO_SHRED_PRESERVES_EVIDENCE       = 27;
  INV_028_AI_CANNOT_ALTER_BOOT_INTEGRITY        = 28;
  INV_029_FOREIGN_SCRIPTS_NEVER_ROOT_ON_HOST    = 29;
  INV_030_WORKSPACE_DATA_BOUNDARY_ISOLATION     = 30;
  INV_031_RENDER_SURFACE_IS_NOT_AUTHORITY       = 31;
  INV_032_FEDERATED_IDENTITY_LOSSLESS_NONESCALATING = 32;
  INV_033_OFFHOST_BACKUP_ENCRYPTED_AND_RECOVERABLE  = 33;
  INV_034_EVIDENCE_TIMESTAMP_DECLARES_TIME_GRADE    = 34;
}
```

| ID      | Title                                                                                    | Owning section | Relation                                |
| ------- | ---------------------------------------------------------------------------------------- | -------------- | --------------------------------------- |
| INV-025 | AI cannot author eBPF                                                                    | S24            | specializes INV-002                     |
| INV-026 | Cluster root cannot override host policy                                                 | S25 / S16.8    | new (fleet)                             |
| INV-027 | Crypto-shred erasure preserves the evidence chain                                        | S16.9          | reconciles INV-005 ↔ GDPR               |
| INV-028 | AI cannot author or alter boot-integrity expectations or boot evidence                   | S16.4          | specializes INV-002 + INV-005           |
| INV-029 | Foreign/maintainer install scripts never execute as root or on the live host             | S21            | specializes INV-002 + INV-012           |
| INV-030 | Workspace data-boundary isolation                                                        | S22            | specializes INV-011                     |
| INV-031 | A render/approval surface is a policy surface, never an authority                        | S23            | specializes INV-002 + INV-009 + INV-019 |
| INV-032 | Federated identity is loss-free and non-escalating                                       | S25            | new (federation)                        |
| INV-033 | Off-host backup is encrypted-at-source and restore preserves the last verified-good copy | S26            | new (DR)                                |
| INV-034 | Every evidence timestamp declares its time-trust grade                                   | S28            | strengthens INV-005                     |

## 3. The new invariants

### INV-025 — AI cannot author eBPF

**Statement:** `AI_NATIVE_SUBJECT` and `AI_AGENT_CAPSULE` subjects cannot author, compile,
sign, or load eBPF programs. An AI may at most request, via a typed action, a pre-vetted,
signed, **drop-only** eBPF template (no `bpf_redirect`, no map writes that reach userspace
control paths). Authoring/loading of eBPF is restricted to `HUMAN_OPERATOR`/`HUMAN_USER`
and, under policy, `SYSTEM_SERVICE` subjects for observability.

**Why:** eBPF runs in-kernel. AI-authored in-kernel code is direct execution and breaks the
propose-not-execute boundary (INV-002).

**Enforced by:** S24 ecosystem runtime adapter (`RUNTIME_EBPF_NATIVE`); S2.3 hard-deny on
eBPF authorship for AI subjects.

**Verified by:** S2.4 scheduled audit — no eBPF load receipt names an AI subject as author.

**Cannot be loosened by:** any policy bundle or capability grant. Per DEC-R3-005.

### INV-026 — Cluster root cannot override host policy

**Statement:** In fleet/cluster mode the host remains sovereign. A cluster root or fleet
controller may distribute baselines, request actions, and read replicated evidence, but it
cannot force a local Policy Kernel to `ALLOW` what host policy denies. A cluster directive is
an input to the host decision, never a substitute for it.

**Why:** federation must not become a remote-root backdoor. Centralized override would void
the local trust boundary and INV-008 default-deny.

**Enforced by:** S25 fleet contract; S16.8 zero-trust posture; S2.3 evaluates cluster
directives as ordinary policy inputs, default-deny on conflict.

**Verified by:** S2.4 audit — every `HOST_POLICY_OVERRIDE_DENIED` path holds; no cluster-origin
decision bypasses local default-deny.

**Cannot be loosened by:** any cluster trust root or delegation. Per DEC-R3-003.

### INV-027 — Crypto-shred erasure preserves the evidence chain

**Statement:** GDPR/RTBF erasure of personal data is performed by destroying the per-subject
data-encryption key (crypto-shredding), not by deleting, modifying, or reordering evidence
records. After erasure the ciphertext is permanently undecryptable; the append-only evidence
chain (INV-005) still verifies; `VerifyChain` still passes.

**Why:** reconciles the GDPR right-to-erasure with the constitutional append-only evidence log.
Deleting evidence to satisfy erasure would void the audit witness.

**Enforced by:** S16.9 data governance; Vault Broker key destruction; S3.1 chain unchanged.

**Verified by:** S2.4 audit — after `ERASURE_EXECUTED_CRYPTO_SHRED`, evidence hash chain intact
AND target ciphertext key absent from the vault.

**Cannot be loosened by:** any policy bundle. Per DEC-R3-006.

### INV-028 — AI cannot author or alter boot-integrity expectations or boot evidence

**Statement:** AI subjects cannot author, sign, or alter boot-integrity expectations
(PCR expectations, IMA policy, firmware pins, dm-verity/IPE root hashes, lockdown level) and
cannot mutate boot-integrity evidence. The boot root of trust is operator/trust-root governed;
an AI may read posture and propose typed remediation for Policy Kernel decision only.

**Why:** the measured-boot root of trust is what every other control attests against. AI
authorship of it would let cognition redefine the floor it runs on (INV-002 + INV-005 in the
boot axis).

**Enforced by:** S16.4 measured boot; recovery-gated boot-policy mutation (INV-012); S2.3
hard-deny on AI boot-policy authorship.

**Verified by:** S2.4 audit — no boot-integrity policy/evidence mutation names an AI subject.

**Cannot be loosened by:** any policy bundle. Per DEC-R3-002.

### INV-029 — Foreign/maintainer install scripts never execute as root or on the live host

**Statement:** Maintainer scripts and foreign package install logic (deb `postinst`, RPM
scriptlets, AUR build functions, vendor `.run`/installer scripts) never execute as root or
against the live host. They are observed in an isolated lab (Universal App Lab / Shadow
Install) and translated into typed AIOS actions; untranslatable scripts are blocked or routed
to a VM.

**Why:** running arbitrary upstream scripts as root is the dominant Linux supply-chain risk and
is incompatible with bounded, evidenced mutation.

**Enforced by:** S21 Package Rosetta / Shadow Install; S3.2 sandbox; S2.3 hard-deny on
host-root script execution.

**Verified by:** S2.4 audit — no `succeeded` host mutation traces to an untranslated foreign
script.

**Cannot be loosened by:** repo trust level or operator convenience.

### INV-030 — Workspace data-boundary isolation

**Statement:** A workload bound to one workspace (work / gaming / lab / family / admin) cannot
read another workspace's data, secrets, or saves. A gaming workspace can never read work or
family data regardless of performance mode or operator convenience.

**Why:** workspace is the desktop-era trust boundary; this is the per-workspace specialization
of INV-011 (cross-group access forbidden).

**Enforced by:** S22 workspace model; S3.2 sandbox filesystem policy; S2.3 hard-deny
(reuses `CrossGroupAccessForbidden` semantics across workspaces).

**Verified by:** S2.4 audit — no cross-workspace read receipt outside an approved, evidenced
export.

**Cannot be loosened by:** any policy bundle, gaming/performance mode, or app manifest.

### INV-031 — A render/approval surface is a policy surface, never an authority

**Statement:** A renderer — including the Mobile and Voice surfaces — carries human consent
bound to one exact action hash (S5.3 `EXACT_ACTION`) into the Policy Kernel. It can never
decide, self-approve, widen scope beyond the bound hash, hold standing admin authority, or
weaken the active SecurityProfile. Voice input is untrusted text subject to the S20
prompt-boundary classifier.

**Why:** renderers are surfaces over typed state; if a surface could grant authority it would
become an uncontrolled admin plane (extends INV-002, INV-009, INV-019/INV-020).

**Enforced by:** S23 mobile/voice contract; S5.3 exact-action binding; S2.3 decision authority;
S7 renderer chrome rules.

**Verified by:** S2.4 audit — every mobile/voice-originated effect resolves to a Policy Kernel
decision on a matching action hash.

**Cannot be loosened by:** any renderer config, theme, or operator setting. Per DEC-R3-004 / DEC-R3-007.

### INV-032 — Federated identity is loss-free and non-escalating

**Statement:** Every legacy single-host subject id resolves to exactly one
`(realm:default, local_id)` and back without ambiguity. A foreign-realm subject can never
resolve to a more privileged local subject than its signed `CrossOrgTrustDelegation` ceiling
permits.

**Why:** federation must not silently escalate privilege or lose identity provenance.

**Enforced by:** S25 federated identity; S5.1 identity model; S2.3 evaluates the delegation
ceiling.

**Verified by:** S2.4 audit — identity resolution is bijective for legacy ids and never exceeds
the delegation ceiling.

**Cannot be loosened by:** any cross-org delegation or cluster root.

### INV-033 — Off-host backup is encrypted-at-source and restore preserves the last verified-good copy

**Statement:** Data leaves the host for backup only encrypted-at-source (the backup target sees
ciphertext + content hashes, never plaintext or key material). A restore never destroys the last
verified-good copy (current state or last `SEALED` BackupSet) before a new copy reaches
`RESTORE_VERIFIED`. `AIRGAP_HIGH` additionally forbids any live-network backup target.

**Why:** backups are a constitutional safety net; an unencrypted or self-destructive backup path
is a data-loss and exfiltration vector.

**Enforced by:** S26 backup/DR; S16.9 crypto-shred composition; S9.1 recovery boundary.

**Verified by:** S2.4 audit — backup payloads are ciphertext-only; a rollback anchor exists until
`RESTORE_VERIFIED`.

**Cannot be loosened by:** any policy bundle. Composes with INV-027 and INV-005.

### INV-034 — Every evidence timestamp declares its time-trust grade

**Statement:** Every evidence record is appended with a known, present `TimeTrustGrade`
(`UNTRUSTED_LOCAL` | `MONOTONIC_ONLY` | `ATTESTED_SINGLE` | `ATTESTED_QUORUM`). The grade is
computed at emit time, is immutable post-seal, and may never be raised retroactively. An
untrusted clock yields honest `UNTRUSTED_LOCAL`/`MONOTONIC_ONLY` evidence, never a silently
trusted timestamp.

**Why:** evidence ordering and TTLs depend on time; an unqualified timestamp is a forgeable
audit claim. This strengthens INV-005 (append-only) on the time axis.

**Enforced by:** S28 constitutional time plane; S3.1 evidence appender records the grade.

**Verified by:** S2.4 audit — no sealed record lacks a `TimeTrustGrade`; no grade was raised
after seal.

**Cannot be loosened by:** any policy bundle or operator clock change.

## 4. Bundle and gating

These ten entries are added to the signed `InvariantBundle` (Rev.2 §4) in a recovery-mode,
`HUMAN_USER` bundle update that emits `INVARIANT_BUNDLE_LOADED` (FOREVER). The degraded-mode
floor is unchanged: signature failure leaves only INV-001 and INV-002 active. Each new
invariant must have its S2.4 verifier wired before any Rev.3 capability impacting it may reach
grade E4 (Rev.2 §6 constraint, extended to INV-025..034).

## 5. Acceptance criteria

1. `InvariantId` enum has 34 values; INV-025..034 added without renumbering INV-001..024.
2. Each new invariant has Statement / Why / Enforced by / Verified by / Cannot be loosened by.
3. Every Rev.3 section (S16–S28) that states a new constitutional rule cites exactly one of
   INV-025..034 (no draft `INV-028` collision remains except S16.4, the legitimate owner).
4. The bundle update path and degraded-mode floor are unchanged from Rev.2.
5. No inherited invariant (INV-001..024) is weakened or re-stated.

## See also

- [Rev.2 Constitutional Invariants (INV-001..024)](../002.AI-OS.NET--SPECREV.2/L0_Governance_Evidence_Safety/04_invariants.md)
- [Rev.3 Design Decisions](02_design_decisions.md)
- [Rev.3 Holistic Specification](00_REV3_HOLISTIC_SPEC.md)
- [Rev.3 Master Index](00_MASTER_INDEX.md)
