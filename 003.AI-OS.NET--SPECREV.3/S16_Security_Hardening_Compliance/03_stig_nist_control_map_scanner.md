# S16.3 — STIG/NIST Control Map + Scanner

| Field     | Value                                                                                                                                           |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-28; awaiting implementation evidence)                                                                               |
| Phase tag | S16.3                                                                                                                                           |
| Layer     | L4/L8/L9/L10 cross-cutting                                                                                                                      |
| Consumes  | S16.1 Security Profile Matrix, S16.2 SELinux MAC Policy Plane, S3.1 Evidence Log, S12.2 Package Model, S8.5 Firmware Trust, S8.1 Network Policy |
| Produces  | control map, `aios-hardening-audit`, checklist exports, exception register                                                                      |

## 1. Purpose

S16.3 makes hardening auditable. It maps AIOS controls to external frameworks,
runs local checks, records evidence, and exports reports for operators and
auditors.

The scanner proves posture. It does not create formal certification by itself.

Invariant links: INV-005, INV-014, INV-015.

## 2. Control map schema

```yaml
control:
  control_id: "AIOS-STIG-AC-0001"
  source: AIOS_NATIVE
  external_refs:
    - source: NIST_800_53
      id: AC-3
    - source: DISA_STIG
      id: "vendor-or-linux-rule-id-when-bound"
    - source: CIS_V8
      id: "6"
  family: AC
  title: "Access enforcement for state-changing actions"
  statement: "State-changing actions must pass the Policy Kernel."
  profiles: [SECURE_DEFAULT, STIG_ALIGNED, AIRGAP_HIGH]
  severity: high
  enforced_by:
    policy_rules: [hd.disable_policy_kernel]
    selinux_domains: [aios_policy_t, aios_runtime_t]
    verification_primitives: [policy_decision_present]
    evidence_records: [POLICY_DECISION, ACTION_RECEIVED]
  scanner:
    check_id: "aios.check.policy_kernel_required"
    probe: "policy_bundle_and_service_probe"
  status: CONTRACT
```

Status values:

| Status     | Meaning                                                                  |
| ---------- | ------------------------------------------------------------------------ |
| `REAL`     | Implemented, scanned, evidenced.                                         |
| `PARTIAL`  | Implemented but incomplete, manually checked, or missing export mapping. |
| `CONTRACT` | Specified but not implemented.                                           |
| `DEFERRED` | Accepted future work; not required for current profile gate.             |

## 3. Scanner architecture

```text
aios-hardening-audit
  -> load active SecurityProfileManifest
  -> load control map
  -> collect host posture probes
  -> collect AIOS service/domain/package posture
  -> evaluate controls
  -> apply exception register
  -> emit HARDENING_CHECK_RESULT evidence
  -> produce reports/exports
```

Probe classes:

| Probe class           | Examples                                                                     |
| --------------------- | ---------------------------------------------------------------------------- |
| Boot posture          | Secure Boot, kernel lockdown, TPM PCR quote, root integrity.                 |
| MAC posture           | SELinux present/enforcing, AIOS domains, forbidden unconfined services.      |
| Service posture       | systemd unit hardening score, privilege flags, socket exposure.              |
| Package posture       | signatures, SBOM, provenance, VEX, repo trust.                               |
| Network posture       | inbound default, outbound manifests, VPN/mTLS/WireGuard posture.             |
| Crypto posture        | active provider, FIPS overlay status, algorithm evidence fields.             |
| App/container posture | Docker socket exposure, rootless mode, privileged containers, device grants. |
| Audit posture         | evidence service running, retention floor, export success.                   |

## 4. Minimum initial control set

| AIOS control id | External family | Requirement                                                                   | Gate |
| --------------- | --------------- | ----------------------------------------------------------------------------- | ---- |
| `AIOS-AC-0001`  | NIST AC         | Every state-changing action passes Policy Kernel.                             | P0   |
| `AIOS-AC-0002`  | NIST AC/IA      | AI subjects cannot self-approve privileged actions.                           | P0   |
| `AIOS-AC-0003`  | NIST AC         | Workspace/app boundaries enforced by policy and sandbox.                      | P0   |
| `AIOS-AU-0001`  | NIST AU         | Evidence Log is append-only and enabled.                                      | P0   |
| `AIOS-AU-0002`  | NIST AU         | Policy decisions, approvals, denials, overrides, and rollbacks are evidenced. | P0   |
| `AIOS-CM-0001`  | NIST CM         | Security profile manifest is signed and active.                               | P0   |
| `AIOS-CM-0002`  | NIST CM         | SELinux policy bundles are signed and rollbackable.                           | P0   |
| `AIOS-CM-0003`  | NIST CM         | systemd services meet profile hardening score floors.                         | P1   |
| `AIOS-IA-0001`  | NIST IA         | Human approval binds to exact action hash.                                    | P0   |
| `AIOS-SC-0001`  | NIST SC         | Default network exposure is denied unless declared.                           | P0   |
| `AIOS-SC-0002`  | NIST SC/FIPS    | FIPS claims require validated module evidence.                                | P1   |
| `AIOS-SI-0001`  | NIST SI         | Secure/measured boot posture is checked and evidenced.                        | P0   |
| `AIOS-SI-0002`  | NIST SI         | Unsigned kernel module load is blocked or recovery-approved.                  | P0   |
| `AIOS-SR-0001`  | NIST SR         | Packages require signatures, SBOM, provenance by profile.                     | P1   |
| `AIOS-SR-0002`  | NIST SR         | Untrusted software is staged/observed before promotion.                       | P0   |

The first implementation may bind DISA STIG rule IDs only where a matching
Linux/product STIG rule is selected. Until then, `external_refs.DISA_STIG` may
remain `UNBOUND` while AIOS-native checks still run.

## 5. Evaluation result

```text
HardeningCheckResult
  check_id
  control_id
  source_refs
  profile_id
  status: PASS | FAIL | WARN | NOT_APPLICABLE | EXCEPTION | NOT_IMPLEMENTED
  severity
  observed_redacted
  expected
  remediation_hint
  exception_id
  evidence_receipt_id
  evaluated_at
```

Promotion gate:

- Any P0 `FAIL` blocks `STIG_ALIGNED` and `AIRGAP_HIGH`.
- P0 `NOT_IMPLEMENTED` blocks `STIG_ALIGNED` until the control is explicitly
  marked `DEFERRED` for that profile.
- `EXCEPTION` passes only while unexpired and approved by an authorized human.

## 6. Exception register

```yaml
exception:
  exception_id: "hex_<ULID>"
  control_id: "AIOS-SI-0001"
  profile_id: STIG_ALIGNED
  reason: "Hardware lacks TPM 2.0"
  compensating_control: "Local-only host, signed boot media, recovery evidence export"
  owner_subject: "human:operator:<id>"
  approved_by: "human:security-admin:<id>"
  approved_at: "2026-05-28T00:00:00Z"
  expires_at: "2026-08-28T00:00:00Z"
  evidence_receipt_id: "evr_..."
  ai_approved: false
```

Exception rules:

1. Exceptions must expire.
2. Exceptions must name an owner.
3. Exceptions must name a compensating control.
4. AI subjects cannot approve exceptions.
5. Expired exceptions revert to `FAIL`.
6. Exceptions are exported with reports.

## 7. Export formats

Required:

- JSON evidence report
- Markdown operator report
- machine-readable control map export

Targeted compatibility:

- OpenSCAP/XCCDF-style checklist export where practical
- ARF-like scanner result export where practical
- STIG Viewer `.cklb` export adapter if the schema remains stable and legally
  safe to generate

OpenSCAP compatibility is an export target, not the internal source of truth.
AIOS evidence remains the internal authority.

## 8. CLI contract

```text
aios-hardening-audit profile show
aios-hardening-audit scan --profile STIG_ALIGNED --format json
aios-hardening-audit scan --profile STIG_ALIGNED --format markdown
aios-hardening-audit export --format xccdf --out ./aios-stig-xccdf.xml
aios-hardening-audit exceptions list
aios-hardening-audit exceptions add --control AIOS-SI-0001 --expires 2026-08-28
```

Every command that mutates exceptions or profile state goes through Policy
Kernel approval and emits evidence.

## 9. Non-goals

- A passing scan is not a formal STIG / NIST / CIS certification; it is alignment evidence only.
- The scanner does not auto-remediate; remediation is a typed action under Policy Kernel decision.
- Control mapping does not claim legal conformity — DoD ATO, FIPS validation, and Common Criteria are separate processes.
- The exception register records waivers with expiry/owner/compensating control; an AI subject can never approve one (INV-002).

## 10. Acceptance criteria

S16.3 is `REAL` only when:

1. Control map loads from signed data and rejects invalid fields.
2. Scanner emits one `HARDENING_CHECK_RESULT` per evaluated control.
3. P0 failures block `STIG_ALIGNED` promotion.
4. Exceptions are expiring, human-approved, evidenced, and reportable.
5. JSON and Markdown reports are generated from the same evaluation result.
6. At least one OpenSCAP/XCCDF-compatible export path is prototyped or explicitly
   documented as not practical for the current control.
7. Recovery mode can run a read-only scan and export evidence without the
   Cognitive Core.
