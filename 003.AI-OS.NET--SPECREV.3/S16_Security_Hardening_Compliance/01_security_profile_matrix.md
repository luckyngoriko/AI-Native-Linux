# S16.1 — Security Profile Matrix

| Field     | Value                                                                                                                             |
| --------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-28; awaiting implementation evidence)                                                                 |
| Phase tag | S16.1                                                                                                                             |
| Layer     | L0/L1/L4/L6/L8/L9/L10 cross-cutting                                                                                               |
| Consumes  | S2.3 Policy Kernel, S9.1 Recovery Boundary, S8.5 Firmware Trust, S3.2 Sandbox Composition, S3.1 Evidence Log, S12.2 Package Model |
| Produces  | `SecurityProfile`, profile manifests, posture gates, profile evidence records                                                     |

## 1. Purpose

`SecurityProfile` fixes the operating security posture of an AIOS host. It
prevents "secure" from being a vague label by binding kernel posture, MAC mode,
crypto boundary, network defaults, package trust, audit retention, update
policy, and exception handling into one explicit contract.

Invariant links: INV-004, INV-005, INV-008, INV-012, INV-013, INV-014,
INV-017.

## 2. Profile enum

```text
SecurityProfile =
  DEV_RELAXED
| SECURE_DEFAULT
| STIG_ALIGNED
| AIRGAP_HIGH
```

`FIPS_STRICT` is not a standalone profile. It is an overlay that may be enabled
only on `STIG_ALIGNED` or `AIRGAP_HIGH` when a validated crypto provider is
available and recorded in evidence.

## 3. Profile matrix

| Dimension                | `DEV_RELAXED`                      | `SECURE_DEFAULT`               | `STIG_ALIGNED`                             | `AIRGAP_HIGH`                         |
| ------------------------ | ---------------------------------- | ------------------------------ | ------------------------------------------ | ------------------------------------- |
| SELinux                  | Permissive allowed                 | Enforcing preferred            | Enforcing required                         | Enforcing required                    |
| Unconfined AIOS services | Blocked except dev fixtures        | Blocked                        | Blocked                                    | Blocked                               |
| Secure Boot              | Optional                           | Recommended                    | Required unless exception                  | Required                              |
| Kernel lockdown          | Optional                           | Integrity when available       | Confidentiality when Secure Boot active    | Confidentiality                       |
| Kernel modules           | Signed preferred                   | Signed by default              | Signed required; recovery-gated exceptions | Signed required; no live exceptions   |
| IMA/EVM/IPE/dm-verity    | Optional                           | Measurement recommended        | Appraisal/immutable root where available   | Required where platform supports      |
| TPM measured boot        | Optional                           | Measured if TPM present        | Required if TPM present                    | Required                              |
| Network default          | Allow LAN/internet with policy     | Default-deny inbound           | Default-deny inbound/outbound exceptions   | Offline/local mirror by default       |
| Package trust            | Dev/untrusted allowed with warning | Verified or reviewed preferred | Verified/signed only unless exception      | Signed local mirror only              |
| App sandbox              | Recommended                        | Required for unknown apps      | Required by app class                      | Required, strongest viable isolation  |
| Containers               | Rootless preferred                 | Rootless default               | Rootless or policy-approved rootful        | Rootless or VM; no Docker socket      |
| Audit retention          | Short                              | Normal                         | Extended                                   | Extended/offline export               |
| Evidence export          | Optional                           | JSON/Markdown                  | JSON/Markdown + checklist exports          | JSON/Markdown + offline audit bundle  |
| AI autonomy              | Dev subject allowed                | AI proposes only               | AI proposes only; stronger approvals       | AI proposes only; no autonomous admin |
| Exceptions               | Free-form allowed                  | Evidence-backed                | Evidence-backed, expiring, owner required  | Rare, recovery-approved               |

## 4. Manifest shape

```yaml
security_profile_manifest:
  profile_id: STIG_ALIGNED
  version: "2026.05.rev3"
  base_release: "aios-rev3"
  overlays:
    fips_strict: false
    airgap_transport: false
  kernel:
    secure_boot_required: true
    lockdown_required: confidentiality
    signed_modules_required: true
    allow_out_of_tree_modules: recovery_approved
  mac:
    backend: selinux
    mode: enforcing
    forbid_unconfined_aios_services: true
  crypto:
    provider: default
    require_fips_validated_module: false
    parallel_sha256_for_fips_evidence: true
  network:
    inbound_default: deny
    outbound_default: deny_unknown
    require_egress_manifests: true
  packages:
    require_signature: true
    require_sbom: true
    require_provenance: true
    allow_untrusted_sources: exception_only
  audit:
    minimum_evidence_grade: E3
    retention_class: extended
    export_formats: [json, markdown, xccdf_when_practical]
  exceptions:
    expiry_required: true
    compensating_control_required: true
    ai_approval_allowed: false
```

## 5. Transition rules

```text
current profile -> requested profile
  -> preflight checks
  -> operator approval if risk increases or weakens controls
  -> profile bundle signature verification
  -> dry-run scanner
  -> apply
  -> emit evidence
  -> run post-apply scanner
  -> promote or rollback
```

Allowed transitions:

| From                 | To               | Gate                                                                   |
| -------------------- | ---------------- | ---------------------------------------------------------------------- |
| `DEV_RELAXED`        | `SECURE_DEFAULT` | Normal operator approval.                                              |
| `SECURE_DEFAULT`     | `STIG_ALIGNED`   | All P0 hardening checks must pass or have approved exceptions.         |
| `STIG_ALIGNED`       | `AIRGAP_HIGH`    | Local mirror, offline evidence export, and recovery path verified.     |
| Any stricter profile | Weaker profile   | Human approval plus downgrade evidence; AI subjects cannot request it. |

## 6. Hard denies

The Policy Kernel must deny these under `STIG_ALIGNED` and `AIRGAP_HIGH`:

| Policy id                               | Denied action                                                               |
| --------------------------------------- | --------------------------------------------------------------------------- |
| `hd.s16.disable_selinux`                | Disable SELinux or switch to permissive.                                    |
| `hd.s16.load_unsigned_mac_policy`       | Load unsigned SELinux policy modules.                                       |
| `hd.s16.unconfined_aios_service`        | Start AIOS-owned service in an unconfined domain.                           |
| `hd.s16.disable_audit`                  | Disable evidence or audit collection.                                       |
| `hd.s16.reduce_audit_retention`         | Reduce retention below profile floor.                                       |
| `hd.s16.install_unsigned_kernel_module` | Install or load unsigned kernel module without recovery-approved exception. |
| `hd.s16.fips_claim_without_module`      | Claim FIPS posture without validated module evidence.                       |
| `hd.s16.untrusted_package_stig`         | Install untrusted package into `STIG_ALIGNED` without exception.            |

## 7. Evidence records

S16.1 adds these evidence record types:

```text
SECURITY_PROFILE_SELECTED
SECURITY_PROFILE_TRANSITION_REQUESTED
SECURITY_PROFILE_TRANSITION_APPLIED
SECURITY_PROFILE_TRANSITION_ROLLED_BACK
HARDENING_EXCEPTION_REGISTERED
HARDENING_EXCEPTION_EXPIRED
HARDENING_CHECK_RESULT
BOOT_INTEGRITY_POSTURE
CRYPTO_BOUNDARY_SELECTED
```

`HARDENING_CHECK_RESULT` is defined canonically in S16.3 §5 (6-state `status`
incl. `NOT_IMPLEMENTED`, singular `control_id`, 12 fields); S16.1 emits it
during profile-transition gating. S16.1 does not restate or diverge from that
schema — see S16.3 §5 for the authoritative field set and the P0
`NOT_IMPLEMENTED` promotion gate.

## 8. Non-goals

- A `SecurityProfile` is not a certification; `STIG_ALIGNED` means "STIG-aligned hardening", never "STIG certified".
- Profile transition never bypasses recovery gating (INV-012) or weakens an inherited invariant (INV-001..024).
- No profile grants an AI subject new authority or self-approval (INV-002, INV-010).
- The matrix declares posture; it does not itself enforce controls — S16.2–.9 do.

## 9. Acceptance criteria

S16.1 is `REAL` only when:

1. All four profiles are represented in signed manifests.
2. Profile validation rejects unknown dimensions and unknown enum values.
3. `STIG_ALIGNED` requires SELinux enforcing in preflight.
4. Downgrades from stricter profiles require human approval.
5. Failed P0 checks block `STIG_ALIGNED` promotion unless an approved exception
   exists.
6. Every transition emits evidence before and after apply.
7. Recovery can display the active profile without the Cognitive Core.
