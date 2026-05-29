# S16.9 — Data Governance, GDPR/RTBF and Audit Export

| Field     | Value                                                                                                                                                         |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-29; awaiting implementation evidence)                                                                                             |
| Phase tag | S16.9                                                                                                                                                         |
| Layer     | L0/L4/L9/L10 cross-cutting                                                                                                                                    |
| Consumes  | S3.1 Evidence Log, S5.2 Vault Broker, S5.1 Identity Model, S16.1 Security Profile Matrix, S16.3 STIG/NIST Control Map + Scanner, S6.3 Evidence Receipt Schema |
| Produces  | `PersonalDataClass`, `CryptoShredKeyRef`, `ErasureRequest`, `ErasureReceipt`, `DataResidencyPolicy`, `AuditExportBundle`, data-governance evidence records    |

## 1. Responsibility

S16.9 defines how AIOS treats personal data as a first-class governed object: how
it is classified, where it may physically reside, how a data subject's right to
erasure (GDPR Art. 17, "right to be forgotten" / RTBF) is honored without
destroying the append-only evidence chain, and how an operator exports an
audit bundle that maps AIOS evidence to external compliance frameworks
(SOC 2, ISO/IEC 27001, HIPAA).

The defining tension this contract resolves is constitutional. INV-005 makes the
evidence log **append-only**: records cannot be deleted, modified, or reordered.
GDPR Art. 17 grants a data subject the right to have their personal data
**erased**. A naive implementation would have to choose one and violate the
other. S16.9 resolves this by **crypto-shredding** (DEC-R3-006): personal data is
never stored in cleartext inside long-lived records; it is encrypted under a
per-subject key, and erasure destroys the _key_, not the _record_. The evidence
chain stays intact and verifiable; the personal payload becomes permanently
unrecoverable. This is captured as new invariant **INV-027**.

Invariant links: INV-003, INV-005, INV-013, INV-015, INV-016, INV-018, INV-027.

## 2. Product principle

The operator must be able to answer three questions truthfully and with evidence:

```text
what personal data does this host hold, and of what class?
where does it physically live, and is that allowed under the active residency policy?
if a subject asks to be forgotten, can I prove it was erased without breaking the audit trail?
```

S16.9 does **not** make AIOS legally compliant. It provides the _technical
controls_ — classification, residency enforcement, crypto-shred erasure,
tamper-evident audit export — that a Data Protection Officer or auditor needs in
order to _demonstrate_ compliance. Legal compliance is a human/organizational
determination, never an AIOS claim (see §13 non-goals).

The product promise mirrors the holistic Rev.3 promise:

```text
classified data
  + explicit residency policy
  + crypto-shred erasure with receipt
  + intact evidence chain
  + exportable audit bundle
  + clear blocked reason when erasure or export cannot proceed
```

## 3. Reference patterns

| Pattern                                                                                                                   | S16.9 use                                                                                 |
| ------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| [GDPR Art. 17 — Right to erasure](https://gdpr-info.eu/art-17-gdpr/)                                                      | The RTBF obligation S16.9 honors via crypto-shredding.                                    |
| [GDPR Art. 30 — Records of processing activities](https://gdpr-info.eu/art-30-gdpr/)                                      | The classification register and `DataResidencyPolicy` provide the technical backing.      |
| [GDPR Art. 32 — Security of processing](https://gdpr-info.eu/art-32-gdpr/)                                                | Per-subject encryption, key custody in the Vault Broker, evidence integrity.              |
| [GDPR Art. 44–49 — Transfers / residency](https://gdpr-info.eu/chapter-5/)                                                | `DataResidencyPolicy` region pinning and the `DATA_RESIDENCY_ENFORCED` gate.              |
| [NIST SP 800-88r1 — Media sanitization (cryptographic erase)](https://csrc.nist.gov/pubs/sp/800/88/r1/final)              | Crypto-shred maps to NIST "Cryptographic Erase" (CE) sanitization.                        |
| [SOC 2 Trust Services Criteria](https://www.aicpa-cima.com/topic/audit-assurance/audit-and-assurance-greater-than-soc-2)  | `AuditExportBundle` maps evidence to the Security/Confidentiality/Privacy criteria.       |
| [ISO/IEC 27001:2022 Annex A](https://www.iso.org/standard/27001)                                                          | Control mapping consumes the S16.3 `ControlMap` and extends it for A.5/A.8 data controls. |
| [HIPAA Security Rule (45 CFR §164.312)](https://www.ecfr.gov/current/title-45/subtitle-A/subchapter-C/part-164/subpart-C) | Audit-controls and integrity reporting fields in the export bundle.                       |

## 4. Personal data classification

Every object that may contain personal data carries a `PersonalDataClass`. The
class drives residency, retention, encryption requirement, and whether the object
is in scope for an erasure request.

```text
PersonalDataClass =
  NON_PERSONAL
| PSEUDONYMOUS_IDENTIFIER
| PERSONAL_BASIC
| PERSONAL_CONTACT
| PERSONAL_BEHAVIORAL
| SPECIAL_CATEGORY_GDPR9
| HEALTH_PHI
| FINANCIAL_PII
| BIOMETRIC
| CHILD_DATA
```

Class semantics:

| Class                     | Meaning                                             | Encryption                             | RTBF in scope |
| ------------------------- | --------------------------------------------------- | -------------------------------------- | ------------- |
| `NON_PERSONAL`            | No identifiable natural person.                     | Optional                               | No            |
| `PSEUDONYMOUS_IDENTIFIER` | Reversible only via a separately held key/map.      | Required (per-subject)                 | Yes           |
| `PERSONAL_BASIC`          | Name, username, device-linked identifier.           | Required (per-subject)                 | Yes           |
| `PERSONAL_CONTACT`        | Email, phone, address.                              | Required (per-subject)                 | Yes           |
| `PERSONAL_BEHAVIORAL`     | Usage/telemetry tied to an identifiable subject.    | Required (per-subject)                 | Yes           |
| `SPECIAL_CATEGORY_GDPR9`  | Art. 9 special-category data.                       | Required (per-subject) + stricter gate | Yes           |
| `HEALTH_PHI`              | Health data (HIPAA / Art. 9 overlap).               | Required (per-subject) + stricter gate | Yes           |
| `FINANCIAL_PII`           | Payment/financial identifiers.                      | Required (per-subject)                 | Yes           |
| `BIOMETRIC`               | Biometric templates (Art. 9).                       | Required (per-subject) + stricter gate | Yes           |
| `CHILD_DATA`              | Data of a minor; elevated consent/erasure handling. | Required (per-subject) + stricter gate | Yes           |

`PersonalDataClass` is a CLOSED enum. Unknown values are rejected by the
data-governance manifest validator. An object whose class is not `NON_PERSONAL`
**must** carry a `CryptoShredKeyRef`; an object that claims `NON_PERSONAL` but is
later observed to contain a personal identifier is a classification defect that
the S16.3 scanner flags as `FAIL` and emits `PERSONAL_DATA_CLASSIFIED` with a
`misclassified` flag.

Classification record shape:

```yaml
personal_data_classification:
  object_ref: "aios-fs://aios/users/<subject_id>/profile"
  data_class: PERSONAL_CONTACT
  subject_id: "subj:user:<canonical>" # S5.1 canonical subject id
  controller_ref: "org:acme" # who is the GDPR controller
  lawful_basis: CONSENT # see LawfulBasis enum below
  key_ref: "csk_<ULID>" # CryptoShredKeyRef.key_ref
  residency_policy_id: "dresp_eu_only"
  classified_by: "subj:system:classifier"
  classified_at: "2026-05-29T10:00:00Z"
  evidence_receipt_id: "evr_..."
```

Lawful basis is itself a CLOSED enum (GDPR Art. 6):

```text
LawfulBasis =
  CONSENT
| CONTRACT
| LEGAL_OBLIGATION
| VITAL_INTEREST
| PUBLIC_TASK
| LEGITIMATE_INTEREST
```

Unknown `LawfulBasis` values are rejected by the data-governance manifest
validator.

## 5. Crypto-shred key model

Personal data is encrypted with a **per-subject data-encryption key (DEK)**. The
DEK itself is never stored in cleartext: it is held by the Vault Broker (S5.2) as
a `VaultCapability` and is itself wrapped by a key-encryption key (KEK). A
`CryptoShredKeyRef` is the durable, non-secret pointer that appears in
evidence and classification records.

```yaml
crypto_shred_key_ref:
  key_ref: "csk_<ULID>"
  subject_id: "subj:user:<canonical>"
  vault_capability_id: "vcap_..." # S5.2 use-without-reveal handle to the DEK
  kek_ref: "kek:aios:datagov:2026" # KEK identity (not the KEK material)
  algorithm: AES_256_GCM # closed enum, see below
  created_at: "2026-05-29T10:00:00Z"
  state: ACTIVE # CryptoShredKeyState
  shredded_at: null
  shred_evidence_id: null
```

Encryption algorithm enum (CLOSED):

```text
DataEncryptionAlgorithm =
  AES_256_GCM
| CHACHA20_POLY1305
| AES_256_GCM_FIPS          # only valid when FIPS_STRICT overlay is active (S16.5)
```

Unknown `DataEncryptionAlgorithm` values are rejected by the key-material loader.
`AES_256_GCM_FIPS` is rejected unless the active profile carries the
`fips_strict` overlay with validated-module evidence (cross-checked against
S16.1 §4 `crypto`).

Key lifecycle FSM:

```text
CryptoShredKeyState =
  PROVISIONED        # key_ref minted, DEK generated in Vault Broker, not yet bound to data
-> ACTIVE            # data encrypted under the DEK exists
-> SHRED_PENDING     # erasure approved; key marked for destruction, reads denied
-> SHREDDED          # DEK + all KEK-wrapped copies destroyed; payload unrecoverable
-> ARCHIVED_REF      # key gone, only the non-secret key_ref remains in the evidence chain
```

Transitions:

```text
PROVISIONED -> ACTIVE          on first encrypt-under-key
ACTIVE      -> SHRED_PENDING   on approved ErasureRequest (policy + human approval)
SHRED_PENDING -> SHREDDED      on Vault Broker destroy-key confirmation
SHREDDED    -> ARCHIVED_REF    on ErasureReceipt emission
```

A `SHREDDED` key can never return to `ACTIVE`. The transition
`SHRED_PENDING -> SHREDDED` is irreversible and is the technical realization of
INV-027.

Crucially, the DEK material is destroyed **inside the Vault Broker** via a
typed use-without-reveal "destroy capability" operation. The data-governance
plane never holds raw key material (INV-003, INV-018); it operates on the
`CryptoShredKeyRef` and the Vault capability handle only.

## 6. Erasure request and receipt (RTBF)

An RTBF request is a typed action, not a free-form deletion. It is decided by the
Policy Kernel (S2.3) and bound to an exact-action human approval (S5.x approval
mechanics); AI subjects can _draft_ and _explain_ an erasure request but can
never approve or execute one (INV-013, INV-016).

```yaml
erasure_request:
  request_id: "erq_<ULID>"
  subject_id: "subj:user:<canonical>"
  requested_by: "subj:human:dpo" # or the data subject via verified channel
  scope: SUBJECT_ALL # ErasureScope enum
  object_refs: [] # required when scope = OBJECT_SET
  legal_ground: ART17_1A_NO_LONGER_NECESSARY # ErasureGround enum
  retention_holds: [] # legal/audit holds that may block erasure
  requested_at: "2026-05-29T10:05:00Z"
  policy_decision_id: "pol_..."
  approval_id: "appr_..."
```

```text
ErasureScope =
  SUBJECT_ALL          # every object whose subject_id matches
| OBJECT_SET           # an explicit set of object_refs
| DATA_CLASS_SUBSET    # all objects of given classes for one subject
```

```text
ErasureGround =
  ART17_1A_NO_LONGER_NECESSARY
| ART17_1B_CONSENT_WITHDRAWN
| ART17_1C_OBJECTION
| ART17_1D_UNLAWFUL_PROCESSING
| ART17_1E_LEGAL_OBLIGATION
| ART17_3_EXEMPTION_DENIED       # request denied under an Art.17(3) exemption
```

Both enums are CLOSED. Unknown values are rejected by the erasure-request
validator.

Erasure execution is the crypto-shred operation: for each `CryptoShredKeyRef` in
scope, the Vault Broker destroys the DEK, the key transitions to `SHREDDED`, and
an `ErasureReceipt` is emitted. The receipt is itself an append-only evidence
record — the _act_ of erasure is recorded forever; only the _personal payload_
is gone.

```yaml
erasure_receipt:
  receipt_id: "errc_<ULID>"
  request_id: "erq_<ULID>"
  subject_id: "subj:user:<canonical>"
  outcome: COMPLETED # ErasureOutcome enum
  keys_shredded:
    - key_ref: "csk_..."
      shredded_at: "2026-05-29T10:06:12Z"
      vault_destroy_evidence_id: "evr_..."
  objects_in_scope: 14
  objects_shredded: 14
  objects_blocked_by_hold: 0
  blocked_reason: null
  evidence_chain_verified: true # VerifyChain over the log still passes (INV-005)
  evidence_receipt_id: "evr_..."
  completed_at: "2026-05-29T10:06:30Z"
```

```text
ErasureOutcome =
  COMPLETED
| PARTIAL_BLOCKED_BY_HOLD     # some objects under legal/audit retention hold
| DENIED_EXEMPTION            # Art.17(3) exemption applied; nothing shredded
| FAILED_VAULT_ERROR          # key destruction could not be confirmed
```

`ErasureOutcome` is CLOSED; unknown values are rejected by the receipt validator.
When a legal/audit hold or an Art. 17(3) exemption blocks erasure, the outcome
must be `PARTIAL_BLOCKED_BY_HOLD` or `DENIED_EXEMPTION` with a non-null
`blocked_reason` — the operator and the subject get an explainable reason, never
a silent no-op.

### 6.1 RTBF ↔ append-only-evidence resolution (explicit)

This is the constitutional crux and is stated here without ambiguity.

```text
INV-005 (append-only): the evidence log is never deleted, modified, or reordered.
GDPR Art. 17 (RTBF):    a subject's personal data must become unrecoverable.

Resolution (DEC-R3-006, INV-027):
  personal data is NEVER stored in cleartext inside an evidence record;
  it is stored encrypted under a per-subject DEK;
  evidence records carry only:
    - the CryptoShredKeyRef (non-secret pointer)
    - ciphertext or a hash/object reference to ciphertext
    - classification, lawful basis, residency, lifecycle metadata
  erasure destroys the DEK inside the Vault Broker;
  the ciphertext remains in place but is permanently undecryptable;
  the evidence chain (hashes, signatures, ordering) is untouched;
  VerifyChain over the log still passes after erasure.
```

Consequences enforced by this contract:

- An evidence record MUST NOT embed cleartext personal data. The S16.3 scanner
  treats a cleartext-personal-data-in-evidence finding as a P0 `FAIL`. This is
  also the existing INV-015 guarantee (evidence never contains secrets) extended
  to personal data.
- Crypto-shred MUST NOT delete, rewrite, or reorder any evidence record. The
  `ErasureReceipt.evidence_chain_verified` field is set only after a successful
  `VerifyChain` (S3.1) run; if `VerifyChain` would fail, erasure is aborted and
  the outcome is `FAILED_VAULT_ERROR`.
- The `ErasureReceipt` and the Vault destroy-key evidence are themselves
  append-only records. Erasure is therefore _fully auditable_: an auditor can
  prove the data was rendered unrecoverable and _when_, without ever recovering
  the data.

## 7. Data residency

`DataResidencyPolicy` pins where personal data of a given class may physically
reside and where audit bundles may be exported. EU GDPR residency is the first
target (DEC-R3-006).

```yaml
data_residency_policy:
  policy_id: "dresp_eu_only"
  primary_region: EU
  allowed_regions: [EU] # closed ResidencyRegion values
  forbidden_regions: [US, APAC, GLOBAL_CDN]
  class_overrides:
    SPECIAL_CATEGORY_GDPR9: { allowed_regions: [EU] }
    HEALTH_PHI: { allowed_regions: [EU] }
  cross_border_transfer:
    permitted: false
    mechanism: NONE # TransferMechanism enum
  export_destinations_allowed: [EU]
```

```text
ResidencyRegion =
  EU
| EEA
| UK
| US
| APAC
| ON_PREM_LOCAL
| GLOBAL_CDN
```

```text
TransferMechanism =
  NONE
| ADEQUACY_DECISION          # GDPR Art. 45
| STANDARD_CONTRACTUAL_CLAUSES   # GDPR Art. 46
| BINDING_CORPORATE_RULES    # GDPR Art. 47
```

Both enums are CLOSED; unknown values are rejected by the residency-policy
loader. A write or replication that would place a personal-data object outside
`allowed_regions` is denied by the Policy Kernel and emits
`DATA_RESIDENCY_ENFORCED` with `decision: DENY`. A permitted cross-border
transfer requires a non-`NONE` `mechanism` recorded in evidence.

Residency interacts with the cluster model (DEC-R3-003): Merkle-DAG evidence
replication across hosts MUST honor the residency policy of the data's class — a
host in a forbidden region may receive evidence-chain _hashes_ (which contain no
personal payload) but MUST NOT receive ciphertext of out-of-region personal data.

## 8. Audit export bundle

An `AuditExportBundle` is a signed, self-contained, tamper-evident package that
maps AIOS evidence to external compliance frameworks. It is read-only over the
evidence log (it never mutates it) and is the artifact an auditor consumes.

```yaml
audit_export_bundle:
  bundle_id: "audex_<ULID>"
  generated_at: "2026-05-29T11:00:00Z"
  generated_by: "subj:human:auditor" # never an AI subject (INV-016)
  profile_id: STIG_ALIGNED # active SecurityProfile at export
  time_range: { from: "2026-01-01T00:00:00Z", to: "2026-05-29T00:00:00Z" }
  frameworks: [SOC2, ISO27001, HIPAA, GDPR] # ComplianceFramework enum set
  evidence_segment_refs: ["seg_...", "seg_..."]
  evidence_chain_root: "blake3:..." # Merkle/hash-chain root proving completeness
  control_map_ref: "ctlmap_rev3" # from S16.3
  framework_mappings:
    SOC2:
      - criterion: "CC6.1" # logical access controls
        aios_controls: ["s16.1.selinux_enforcing", "s5.2.vault_no_reveal"]
        evidence_record_types:
          [HARDENING_CHECK_RESULT, CRYPTO_BOUNDARY_SELECTED]
        status: SUPPORTED
      - criterion: "C1.2" # confidentiality / disposal
        aios_controls: ["s16.9.crypto_shred"]
        evidence_record_types: [ERASURE_EXECUTED_CRYPTO_SHRED]
        status: SUPPORTED
    ISO27001:
      - control: "A.8.10" # information deletion
        aios_controls: ["s16.9.crypto_shred"]
        evidence_record_types: [ERASURE_EXECUTED_CRYPTO_SHRED]
        status: SUPPORTED
      - control: "A.5.34" # privacy & PII protection
        aios_controls: ["s16.9.classification", "s16.9.residency"]
        evidence_record_types:
          [PERSONAL_DATA_CLASSIFIED, DATA_RESIDENCY_ENFORCED]
        status: SUPPORTED
    HIPAA:
      - safeguard: "164.312(b)" # audit controls
        aios_controls: ["s3.1.append_only", "s16.3.scanner"]
        evidence_record_types: [HARDENING_CHECK_RESULT]
        status: SUPPORTED
      - safeguard: "164.312(c)(1)" # integrity
        aios_controls: ["s3.1.hash_chain"]
        evidence_record_types: [AUDIT_BUNDLE_EXPORTED]
        status: SUPPORTED
    GDPR:
      - article: "Art.17"
        aios_controls: ["s16.9.crypto_shred"]
        evidence_record_types: [ERASURE_EXECUTED_CRYPTO_SHRED]
        status: SUPPORTED
      - article: "Art.30"
        aios_controls: ["s16.9.classification"]
        evidence_record_types: [PERSONAL_DATA_CLASSIFIED]
        status: SUPPORTED
  signature: "ed25519:..." # bundle signature; verifiable offline
  export_format: [json, markdown]
```

```text
ComplianceFramework =
  SOC2
| ISO27001
| HIPAA
| GDPR
| NIST_800_53        # reuses S16.3 control map
| CIS                # reuses S16.3 control map
```

```text
MappingStatus =
  SUPPORTED              # AIOS provides a technical control + evidence for this criterion
| PARTIAL               # control exists but evidence coverage is incomplete
| NOT_APPLICABLE
| OUT_OF_SCOPE          # framework criterion is organizational, not technical
```

Both enums are CLOSED; unknown values are rejected by the bundle validator. The
bundle reuses the S16.3 `ControlMap` for NIST/CIS criteria rather than
re-deriving them. The bundle's `framework_mappings` describe _technical control
support and evidence pointers only_; `status: SUPPORTED` asserts that a control
and its evidence exist, never that the organization is certified.

Export integrity:

- The bundle carries `evidence_chain_root`; a verifier recomputes the chain over
  `evidence_segment_refs` and confirms it matches before trusting any mapping.
- The bundle is signed (Ed25519). Under `AIRGAP_HIGH` the bundle is exported as
  an offline package (no network destination).
- Export itself emits `AUDIT_BUNDLE_EXPORTED` evidence — the audit trail records
  that an audit bundle left the system, by whom, and over what range.

## 9. Security profile gates

| Profile          | Data-governance rule                                                                                                                                                                                                     |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `DEV_RELAXED`    | Classification optional with warning; crypto-shred available; residency advisory only; export allowed unsigned for dev fixtures.                                                                                         |
| `SECURE_DEFAULT` | Personal-data classes require a `CryptoShredKeyRef`; RTBF requires policy + human approval; residency enforced; export signed.                                                                                           |
| `STIG_ALIGNED`   | All personal data encrypted per-subject; residency enforced with deny on violation; RTBF requires expiring, owner-bound approval; export signed + checklist mapping; cleartext-personal-data-in-evidence is a P0 `FAIL`. |
| `AIRGAP_HIGH`    | As `STIG_ALIGNED` plus: no cross-border transfer (`mechanism` must be `NONE`); export only as offline signed bundle; key custody local Vault only; `GLOBAL_CDN`/`US` residency forbidden.                                |

Hard denies (Policy Kernel) under `STIG_ALIGNED` and `AIRGAP_HIGH`:

| Policy id                                 | Denied action                                                                    |
| ----------------------------------------- | -------------------------------------------------------------------------------- |
| `hd.s16_9.cleartext_personal_in_evidence` | Write cleartext personal data into an evidence record.                           |
| `hd.s16_9.ai_approve_erasure`             | AI subject approving or executing an `ErasureRequest`.                           |
| `hd.s16_9.delete_evidence_for_rtbf`       | Deleting/rewriting evidence records to satisfy RTBF (must crypto-shred instead). |
| `hd.s16_9.residency_violation`            | Placing/replicating personal data outside `allowed_regions`.                     |
| `hd.s16_9.export_without_signature`       | Exporting an `AuditExportBundle` without a valid signature.                      |
| `hd.s16_9.unwrapped_dek_read`             | Reading a DEK as raw material instead of via Vault use-without-reveal.           |

`FIPS_STRICT` overlay (S16.5): when active, the only admissible
`DataEncryptionAlgorithm` is `AES_256_GCM_FIPS`, backed by validated-module
evidence; other algorithms are rejected for personal-data classes.

## 10. Evidence records

S16.9 adds these evidence record types:

```text
PERSONAL_DATA_CLASSIFIED
DATA_RESIDENCY_ENFORCED
CRYPTO_SHRED_KEY_PROVISIONED
ERASURE_REQUESTED
ERASURE_APPROVED
ERASURE_EXECUTED_CRYPTO_SHRED
ERASURE_BLOCKED_BY_HOLD
ERASURE_DENIED_EXEMPTION
AUDIT_BUNDLE_EXPORTED
DATA_GOVERNANCE_EXCEPTION_REGISTERED
```

`ERASURE_EXECUTED_CRYPTO_SHRED` minimum fields:

```text
request_id
subject_id
key_refs_shredded
vault_destroy_evidence_ids
objects_in_scope
objects_shredded
objects_blocked_by_hold
outcome
evidence_chain_verified            # MUST be true for outcome COMPLETED
security_profile
approved_by
approval_id
evidence_receipt_id
```

Every record in this contract is append-only (INV-005) and contains no cleartext
personal data and no secret material (INV-015, INV-018). `key_refs_shredded`
holds `CryptoShredKeyRef.key_ref` pointers, never DEK material.

## 11. Solver alignment

Data-governance operations follow the universal Rev.3 solver pattern (holistic
§6); S16.9 does not invent a parallel one. An RTBF request flows as:

```text
erasure request (typed action)
  -> inspect signed state (classification register + CryptoShredKeyRef set + residency policy)
  -> generate candidate scope (which objects/keys match subject_id)
  -> score: completeness vs legal holds vs exemptions  (benefit/risk/compat)
  -> show risk diff: "N objects, M under hold, K exemptions; chain stays intact"
  -> apply policy + exact-action human approval
  -> execute crypto-shred in Vault Broker (off-path of the evidence chain)
  -> verify: VerifyChain still passes; keys confirmed destroyed
  -> promote with ErasureReceipt evidence
  -> on failure: block with reason (FAILED_VAULT_ERROR / PARTIAL_BLOCKED_BY_HOLD)
```

Audit export follows the same shape: inspect signed evidence segments, generate
the framework mapping candidate, verify the chain root, sign and promote with
`AUDIT_BUNDLE_EXPORTED`, or block if the chain does not verify.

## 12. Operator UX

The operator (or DPO) sees a Data Governance panel, not raw logs:

- per-class inventory: how many objects of each `PersonalDataClass` exist
- residency status: where each class lives vs the active `DataResidencyPolicy`
- RTBF queue: pending erasure requests, who must approve, what is blocked and why
- erasure receipt: "Subject X erased on date Y; 14/14 objects crypto-shredded;
  evidence chain verified intact"
- export action: generate signed audit bundle for a framework + time range
- blocked reason: explicit, e.g. "3 objects under legal hold until 2027-01-01"

One-click operator actions (each maps to a typed policy decision; the UI is not
authority):

```text
Classify object
Request erasure (RTBF)
Approve erasure
Export audit bundle
Set residency policy
Register data-governance exception
```

## 13. Non-goals

- Do not claim AIOS is GDPR/SOC 2/ISO 27001/HIPAA _certified_ or _compliant_.
  S16.9 provides technical controls and audit material; compliance is a legal and
  organizational determination made by humans.
- Do not satisfy RTBF by deleting, rewriting, or reordering evidence records.
  Erasure is always crypto-shred (key destruction), never log mutation.
- Do not store cleartext personal data inside any evidence or long-lived record.
- Do not let an AI subject classify-as-non-personal to evade erasure, approve an
  erasure, or sign an audit bundle.
- Do not treat the DEK as a readable value; key custody is the Vault Broker's
  (S5.2) and operations are use-without-reveal.
- Do not promise that crypto-shred recovers storage space; it renders payload
  undecryptable, it does not necessarily reclaim blocks.
- Do not replicate out-of-region personal-data ciphertext under the cluster
  Merkle-DAG; only chain hashes (no payload) may cross a forbidden region.

## 14. Acceptance criteria

S16.9 is `REAL` only when:

1. `PersonalDataClass`, `LawfulBasis`, `DataEncryptionAlgorithm`,
   `CryptoShredKeyState`, `ErasureScope`, `ErasureGround`, `ErasureOutcome`,
   `ResidencyRegion`, `TransferMechanism`, `ComplianceFramework`, and
   `MappingStatus` validators reject unknown enum values.
2. Any object whose class is not `NON_PERSONAL` is rejected at write time unless
   it carries a valid `CryptoShredKeyRef`.
3. An `ErasureRequest` cannot transition a key to `SHREDDED` without a Policy
   Kernel decision plus a bound human approval; AI-subject attempts are denied
   and recorded.
4. Crypto-shred destroys the DEK in the Vault Broker and the matching
   `CryptoShredKeyRef` reaches `SHREDDED`/`ARCHIVED_REF`; the encrypted payload is
   thereafter undecryptable.
5. After any erasure, `VerifyChain` (S3.1) over the evidence log still passes;
   no record was deleted, modified, or reordered (INV-005, INV-027).
6. `ERASURE_EXECUTED_CRYPTO_SHRED` is emitted with `evidence_chain_verified:true`
   only when the post-erasure chain verification actually passed.
7. A residency-violating write or replication is denied and emits
   `DATA_RESIDENCY_ENFORCED` with `decision: DENY`.
8. No evidence record contains cleartext personal data; the S16.3 scanner reports
   a P0 `FAIL` if one is found.
9. An `AuditExportBundle` is signed, carries a verifiable `evidence_chain_root`,
   maps at least SOC 2 / ISO 27001 / HIPAA / GDPR criteria to AIOS evidence record
   types, and emits `AUDIT_BUNDLE_EXPORTED`; export by an AI subject is rejected.
10. Erasure blocked by a legal hold or Art. 17(3) exemption returns
    `PARTIAL_BLOCKED_BY_HOLD`/`DENIED_EXEMPTION` with a non-null
    `blocked_reason`, never a silent no-op.

## 15. See also

- [S3.1 Evidence Log](../../002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/01_evidence_log.md)
- [S6.3 Evidence Receipt Schema](../../002.AI-OS.NET--SPECREV.2/L0_Governance_Evidence_Safety/03_evidence_receipt_schema.md)
- [S5.2 Vault Broker](../../002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/02_vault_broker.md)
- [S5.1 Identity Model](../../002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/03_identity_model.md)
- [S16.1 Security Profile Matrix](01_security_profile_matrix.md)
- [S16.3 STIG/NIST Control Map + Scanner](03_stig_nist_control_map_scanner.md)
- [Rev.3 Design Decisions (DEC-R3-006)](../02_design_decisions.md)
- [Rev.3 Holistic Specification](../00_REV3_HOLISTIC_SPEC.md)
