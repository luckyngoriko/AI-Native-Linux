# S16.6 — SBOM + Provenance + VEX

| Field     | Value                                                                                                                                                                 |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-29; awaiting implementation evidence)                                                                                                     |
| Phase tag | S16.6                                                                                                                                                                 |
| Layer     | L10 (supply-chain trust; crosses L0/L4/L9/L10)                                                                                                                        |
| Consumes  | S12.2 Package Model, S11.1 Repository Model (trust roots), S3.1 Evidence Log, S16.1 Security Profile Matrix, S16.3 STIG/NIST Control Map                              |
| Produces  | `AiosSbom`, `SbomComponent`, `ProvenanceAttestation`, `VexStatement`, `ReproducibleBuildReceipt`, `SupplyChainRiskScore`, `PackagePassport.supply_chain` field schema |

## 1. Responsibility

S16.6 defines the supply-chain evidence layer for every artifact AIOS ships,
installs, or admits: packages, kernel builds, kernel/backend adapters, driver
capsules, app capsules, and AI model artifacts. It gives auditors and the
scanner four machine-readable, signature-bound truths per release:

```text
what is inside it        -> SBOM (Software Bill of Materials)
where it came from       -> SLSA-style provenance attestation
whether it is vulnerable -> signed VEX statement
whether it is reproducible -> ReproducibleBuildReceipt
```

S16.6 owns the **schema** of the `PackagePassport.supply_chain` block. It does
**not** own the `PackagePassport` object as a whole — that object is defined by
S21 (Package Rosetta and Universal App Lab). S16.6 fills in the supply-chain
fields with real schemas so S21, S12.2, and the S16.3 scanner can all read one
canonical model.

S16.6 is the schema and validation contract behind control **`AIOS-SR-0001`**
("Packages require signatures, SBOM, provenance by profile") declared in S16.3
§4. The scanner validation rules in §10 below are the concrete probe behind that
control id.

Invariant links: INV-005, INV-013, INV-014, INV-015, INV-017.

## 2. Product principle

AIOS treats supply-chain metadata as evidence, not as decoration.

```text
artifact arrives
  -> normalize SBOM (SPDX or CycloneDX) into one AIOS internal model
  -> verify provenance attestation against trust roots
  -> record signed VEX statements
  -> verify reproducible-build receipt where required
  -> score supply-chain risk
  -> attach all of it to the PackagePassport
  -> let the profile gate decide: admit, warn, or block with reason
```

The operator sees a clear supply-chain verdict ("signed, reproducible, no known
exploitable CVE, low risk") instead of raw JSON. No artifact reaches
`STIG_ALIGNED` or `AIRGAP_HIGH` without the evidence this contract requires.

A passport that merely _claims_ "SBOM present" is not enough. The SBOM must
parse into the internal model, the provenance must verify against a trust root,
and unknown enum values are rejected. False supply-chain claims are themselves a
capability-lie (S11 deplatforming territory) and are recorded as evidence.

## 3. Reference patterns

| Pattern                                                                                   | S16.6 use                                                                            |
| ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| [SPDX](https://spdx.dev/about/overview/)                                                  | One of two accepted external SBOM formats; normalized into `AiosSbom`.               |
| [SPDX 3.0 model](https://spdx.github.io/spdx-spec/v3.0.1/)                                | Component/relationship vocabulary mapped into the internal model.                    |
| [CycloneDX](https://cyclonedx.org/capabilities/)                                          | Second accepted external SBOM format; carries native VEX and component pedigree.     |
| [CycloneDX VEX](https://cyclonedx.org/capabilities/vex/)                                  | Native VEX representation; mapped to `VexStatement`.                                 |
| [SLSA v1.2](https://slsa.dev/spec/v1.2/about)                                             | Provenance attestation model and build-level (L0–L3) semantics.                      |
| [in-toto attestation](https://github.com/in-toto/attestation/blob/main/spec/v1/README.md) | Envelope + predicate structure for `ProvenanceAttestation`.                          |
| [Sigstore / cosign](https://docs.sigstore.dev/)                                           | Signature transport for SBOM, provenance, and VEX; transparency-log inclusion proof. |
| [OpenVEX](https://github.com/openvex/spec)                                                | Minimal VEX predicate vocabulary; mapped to `VexStatement.status`.                   |
| [CSAF 2.0 VEX profile](https://docs.oasis-open.org/csaf/csaf/v2.0/csaf-v2.0.html)         | Export-target compatibility for downstream auditors.                                 |
| [Reproducible Builds](https://reproducible-builds.org/docs/)                              | Bit-for-bit rebuild discipline behind `ReproducibleBuildReceipt`.                    |

These are normalization sources. The AIOS internal model in §4–§8 is the
authority; external formats are accepted on input and produced on export.

## 4. Canonical SBOM model

AIOS **accepts SPDX and CycloneDX on input** and normalizes both into one
internal model, `AiosSbom`. Components, relationships, and licenses are mapped;
unmappable fields are preserved verbatim in `raw_extras` but never trusted for
gating decisions.

```text
SbomFormat =
  SPDX_2_3
| SPDX_3_0
| CYCLONEDX_1_5
| CYCLONEDX_1_6
```

```text
SbomComponentType =
  APPLICATION
| LIBRARY
| FRAMEWORK
| OPERATING_SYSTEM
| KERNEL_IMAGE
| KERNEL_MODULE
| FIRMWARE
| CONTAINER_IMAGE
| AI_MODEL
| DATA_FILE
| SOURCE
```

```yaml
aios_sbom:
  sbom_id: "sbom_<ULID>"
  artifact_ref: "pkg:aios/<name>@<version>" # canonical purl for the subject
  source_format: CYCLONEDX_1_6
  normalized_at: "2026-05-29T00:00:00Z"
  document_hash: "sha256:..." # hash of the original document as received
  document_signature:
    scheme: sigstore | minisign | x509 | UNSIGNED
    signer_ref: "trust-root-ref or UNKNOWN"
    transparency_log_inclusion: true | false
  primary_component:
    bom_ref: "comp-0"
    type: APPLICATION
    name: "<name>"
    version: "<version>"
    purl: "pkg:..."
  components:
    - bom_ref: "comp-1"
      type: LIBRARY
      name: "<dep-name>"
      version: "<dep-version>"
      purl: "pkg:..."
      licenses: ["SPDX-license-id"]
      hashes: ["sha256:..."]
      supplier: "<org or UNKNOWN>"
  relationships:
    - from: "comp-0"
      to: "comp-1"
      kind: DEPENDS_ON
  completeness: COMPLETE | INCOMPLETE | UNKNOWN_DEPTH
  raw_extras: {} # preserved, never gating
```

```text
SbomRelationshipKind =
  DEPENDS_ON
| CONTAINS
| BUILD_DEPENDENCY
| DEV_DEPENDENCY
| RUNTIME_DEPENDENCY
| GENERATED_FROM
| DESCRIBES
```

`SbomFormat`, `SbomComponentType`, and `SbomRelationshipKind` are **closed
enums**. Unknown values are rejected by the SBOM normalizer (`aios-sbom-normalize`);
the artifact is treated as having no valid SBOM.

A component without a `purl` **and** without at least one `hash` is rejected for
`STIG_ALIGNED`/`AIRGAP_HIGH` gating because it cannot be correlated to a
vulnerability feed; it may pass with a `WARN` under `SECURE_DEFAULT`.

## 5. Provenance attestation model

`ProvenanceAttestation` is a SLSA-style, in-toto-shaped statement of how the
artifact was built and by whom. It must verify against a trust root from S11.1
before any field is trusted.

```text
SlsaBuildLevel =
  SLSA_L0   # no provenance
| SLSA_L1   # provenance exists, may be unsigned
| SLSA_L2   # signed provenance, hosted/authenticated build service
| SLSA_L3   # hardened, isolated, non-falsifiable build platform
```

```yaml
provenance_attestation:
  attestation_id: "prov_<ULID>"
  subject:
    artifact_ref: "pkg:aios/<name>@<version>"
    digest: "sha256:..." # MUST equal the admitted artifact digest
  predicate_type: "https://slsa.dev/provenance/v1"
  build:
    builder_id: "aios-builder | lvfs | distro | vendor | local"
    build_type_uri: "https://aios/build-types/hermetic-v1"
    build_level: SLSA_L3
    invocation_hash: "sha256:..." # parameters/config of the build
    started_at: "2026-05-29T00:00:00Z"
    finished_at: "2026-05-29T00:00:00Z"
  materials: # inputs consumed by the build
    - uri: "git+https://.../repo@<commit>"
      digest: "sha1:<commit> | sha256:..."
  signature:
    scheme: sigstore | minisign | x509
    signer_ref: "trust-root-ref"
    transparency_log_inclusion: true | false
  verification:
    trust_root_id: "s11.1-trust-root-ref"
    subject_digest_matches: true | false
    signature_valid: true | false
    verified_at: "2026-05-29T00:00:00Z"
```

Verification rule:

```text
provenance is VERIFIED only if
  signature_valid == true
  and subject.digest == admitted artifact digest
  and signer chains to an S11.1 trust root
  and build_level satisfies the active profile floor (see §9)
```

A provenance whose `subject.digest` does not equal the artifact it is attached to
is a hard reject (digest-confusion attack), recorded as a capability-lie signal.

`SlsaBuildLevel` and the `builder_id`/`scheme` value sets are **closed enums**.
Unknown values are rejected by the provenance verifier (`aios-provenance-verify`).

## 6. VEX statement model

A `VexStatement` declares, for one vulnerability against one artifact, whether
that vulnerability actually affects the shipped product. VEX prevents
"every-CVE-is-a-blocker" noise while keeping the verdict signed and auditable.

```text
VexStatus =
  NOT_AFFECTED
| AFFECTED
| FIXED
| UNDER_INVESTIGATION
```

```text
VexJustification =                                  # required when status == NOT_AFFECTED
  COMPONENT_NOT_PRESENT
| VULNERABLE_CODE_NOT_PRESENT
| VULNERABLE_CODE_NOT_IN_EXECUTE_PATH
| VULNERABLE_CODE_CANNOT_BE_CONTROLLED_BY_ADVERSARY
| INLINE_MITIGATIONS_ALREADY_EXIST
```

```yaml
vex_statement:
  vex_id: "vex_<ULID>"
  artifact_ref: "pkg:aios/<name>@<version>"
  vulnerability:
    id: "CVE-2026-00000 | GHSA-... | AIOS-VULN-..."
    aliases: []
  affected_components: ["comp-1"] # bom_refs from the SBOM
  status: NOT_AFFECTED
  justification: VULNERABLE_CODE_NOT_IN_EXECUTE_PATH # required iff status == NOT_AFFECTED
  impact_statement: "redacted human-readable rationale"
  action_statement: "required iff status == AFFECTED"
  issuer:
    issuer_ref: "vendor | aios-security | distro | operator"
    signature:
      scheme: sigstore | minisign | x509
      signer_ref: "trust-root-ref"
  first_issued_at: "2026-05-29T00:00:00Z"
  last_updated_at: "2026-05-29T00:00:00Z"
  trust:
    signature_valid: true | false
    issuer_authorized_for_artifact: true | false
```

Conditional-field rules enforced by the VEX loader (`aios-vex-validate`):

```text
status == NOT_AFFECTED  => justification REQUIRED, action_statement FORBIDDEN
status == AFFECTED      => action_statement REQUIRED
status == FIXED         => a fixing version/build MUST be referenced
status == UNDER_INVESTIGATION => no gating relief granted
```

A `NOT_AFFECTED` statement that is unsigned, or whose `issuer` is not authorized
for the artifact, is treated as **absent** — the underlying vulnerability is
re-counted as live for gating. AI subjects cannot author or sign VEX statements;
a `VexStatement` whose issuer resolves to an AI subject is rejected.

`VexStatus` and `VexJustification` are **closed enums**. Unknown values are
rejected by the VEX loader.

## 7. ReproducibleBuildReceipt

`ReproducibleBuildReceipt` records an independent rebuild and whether it produced
a bit-identical (or normalized-identical) artifact. It is the strongest available
evidence that the shipped binary matches the attested source.

```text
ReproStatus =
  BIT_IDENTICAL
| NORMALIZED_IDENTICAL        # identical after stripping known non-determinism (timestamps, paths)
| NOT_REPRODUCIBLE
| NOT_ATTEMPTED
```

```yaml
reproducible_build_receipt:
  receipt_id: "rbr_<ULID>"
  artifact_ref: "pkg:aios/<name>@<version>"
  claimed_digest: "sha256:..." # digest from provenance/SBOM
  rebuild:
    rebuilder_id: "aios-rebuilder | third-party-ref"
    build_type_uri: "https://aios/build-types/hermetic-v1"
    environment_hash: "sha256:..." # toolchain + base image lock
    started_at: "2026-05-29T00:00:00Z"
    finished_at: "2026-05-29T00:00:00Z"
  result:
    status: BIT_IDENTICAL
    rebuilt_digest: "sha256:..."
    normalization_applied: [] # rules used when NORMALIZED_IDENTICAL
    diffoscope_summary_ref: "evr_... | none"
  signature:
    scheme: sigstore | minisign | x509
    signer_ref: "trust-root-ref"
```

Verification rule:

```text
reproducible build VERIFIED only if
  status in { BIT_IDENTICAL, NORMALIZED_IDENTICAL }
  and rebuilt_digest == claimed_digest (after declared normalization)
  and receipt signature chains to an S11.1 trust root
```

`ReproStatus` is a **closed enum**. Unknown values are rejected by the receipt
validator (`aios-repro-verify`).

## 8. PackagePassport.supply_chain field schema

S21 owns `PackagePassport`. S16.6 owns the `supply_chain` block inside it. This
is the canonical shape S12.2, S21, and the S16.3 scanner read. S16.6 does not
redefine the rest of the passport.

```yaml
package_passport:
  # ... identity / source / trust / risk / compatibility fields owned by S21 ...
  supply_chain: # owned by S16.6
    sbom_ref: "sbom_<ULID> | none"
    sbom_format_in: SPDX_3_0 | CYCLONEDX_1_6 | none
    provenance_ref: "prov_<ULID> | none"
    slsa_build_level: SLSA_L3 | SLSA_L0
    vex_refs: ["vex_<ULID>"]
    reproducible_build_ref: "rbr_<ULID> | none"
    repro_status: BIT_IDENTICAL | NOT_ATTEMPTED
    risk_score_ref: "scrs_<ULID>"
    supply_chain_verdict: ADMIT | WARN | BLOCK
    verdict_profile: STIG_ALIGNED
    evidence_receipt_ids: ["evr_..."]
```

`supply_chain_verdict` is computed, never operator-asserted, and is always bound
to the profile under which it was computed (`verdict_profile`). Re-evaluating
under a stricter profile may flip `ADMIT` to `BLOCK`; it never silently relaxes.

## 9. Supply-chain risk scoring

`SupplyChainRiskScore` rolls the four artifacts above into one bounded score and
a verdict. It is produced by the scanner, attached to the passport, and emitted
as evidence.

```yaml
supply_chain_risk_score:
  score_id: "scrs_<ULID>"
  artifact_ref: "pkg:aios/<name>@<version>"
  profile_id: STIG_ALIGNED
  inputs:
    sbom_completeness: COMPLETE | INCOMPLETE | UNKNOWN_DEPTH | ABSENT
    provenance_level: SLSA_L3
    signature_state: SIGNED_TRUSTED | SIGNED_UNTRUSTED | UNSIGNED
    repro_status: BIT_IDENTICAL
    open_exploitable_vulns: 0 # AFFECTED/UNDER_INVESTIGATION not relieved by VEX
    known_bad_match: false
  risk_band: LOW | MEDIUM | HIGH | CRITICAL
  verdict: ADMIT | WARN | BLOCK
  blocked_reason: "string or none"
  evidence_receipt_id: "evr_..."
```

```text
SignatureState = SIGNED_TRUSTED | SIGNED_UNTRUSTED | UNSIGNED
RiskBand       = LOW | MEDIUM | HIGH | CRITICAL
```

Both are **closed enums**; unknown values are rejected by the scorer
(`aios-supply-chain-score`).

The scoring solver reuses the universal Rev.3 solver pattern (holistic §6):
inspect the signed supply-chain state, score benefit/risk/compatibility, produce
a risk diff, hand the verdict to the Policy Kernel, and let the profile gate
admit, warn, or block with reason. S16.6 does not invent a parallel solver.

## 10. Scanner validation rules (control AIOS-SR-0001)

These are the concrete probe rules behind `AIOS-SR-0001` (S16.3 §4). The scanner
emits one `HARDENING_CHECK_RESULT` per artifact evaluated.

| Rule id                               | Check                                                | `SECURE_DEFAULT` | `STIG_ALIGNED`         | `AIRGAP_HIGH`                     |
| ------------------------------------- | ---------------------------------------------------- | ---------------- | ---------------------- | --------------------------------- |
| `sr0001.sbom_present`                 | SBOM normalizes into `AiosSbom`                      | WARN if absent   | FAIL if absent         | FAIL if absent                    |
| `sr0001.sbom_components_correlatable` | every gating component has purl or hash              | WARN             | FAIL                   | FAIL                              |
| `sr0001.provenance_verified`          | provenance verifies vs S11.1 trust root              | WARN if absent   | FAIL if absent/invalid | FAIL if absent/invalid            |
| `sr0001.slsa_level_floor`             | build level ≥ profile floor                          | floor `SLSA_L1`  | floor `SLSA_L2`        | floor `SLSA_L2` (local)           |
| `sr0001.signed_locally`               | signature is locally verifiable without network      | not required     | recommended            | **required**                      |
| `sr0001.vex_signed`                   | VEX relieving a CVE is signed + issuer authorized    | WARN             | FAIL                   | FAIL                              |
| `sr0001.no_open_exploitable`          | open `AFFECTED` vulns not relieved by valid VEX      | WARN             | FAIL on HIGH/CRITICAL  | FAIL on any                       |
| `sr0001.reproducible`                 | repro receipt `BIT_IDENTICAL`/`NORMALIZED_IDENTICAL` | not required     | recommended            | required for AIOS-built artifacts |

The scanner runs read-only and can execute in recovery mode without the
Cognitive Core (consistent with S16.3 §9). Validation never mutates the
artifact; it only attaches verdicts and emits evidence.

## 11. Security profile gates

| Profile          | Supply-chain rule                                                                                                                                                                                                                                                                |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `DEV_RELAXED`    | SBOM/provenance/VEX optional; missing metadata produces a warning and is recorded, never silently dropped.                                                                                                                                                                       |
| `SECURE_DEFAULT` | SBOM and verified provenance preferred; missing metadata is `WARN`; signed VEX honored.                                                                                                                                                                                          |
| `STIG_ALIGNED`   | **SBOM + verified provenance required** (`SLSA_L2`+); unsigned VEX is ignored; open HIGH/CRITICAL unrelieved vulns block; reproducible build recommended.                                                                                                                        |
| `AIRGAP_HIGH`    | **All artifacts must be locally signed and locally verifiable** — no live registry, no online transparency-log lookup at admission; provenance and VEX must validate against the offline mirror's pinned trust roots; AIOS-built artifacts require a reproducible-build receipt. |

Hard denies (enforced by the Policy Kernel under `STIG_ALIGNED`/`AIRGAP_HIGH`):

- `hd.s16_6.admit_without_sbom` — admit a package with no normalizable SBOM.
- `hd.s16_6.admit_unverified_provenance` — admit with absent or invalid provenance.
- `hd.s16_6.honor_unsigned_vex` — let an unsigned/unauthorized VEX relieve a CVE.
- `hd.s16_6.airgap_online_admission` — perform online supply-chain lookup at admission under `AIRGAP_HIGH`.
- `hd.s16_6.ai_authored_supply_chain` — accept an SBOM/provenance/VEX/receipt whose issuer resolves to an AI subject.
- `hd.s16_6.digest_mismatch_admit` — admit when provenance/receipt subject digest ≠ artifact digest.

AI subjects (`AI_NATIVE_SUBJECT`, `AI_AGENT_CAPSULE`) may **read, summarize, and
explain** supply-chain evidence and may **propose** a risk verdict, but cannot
author, sign, approve, or override any supply-chain record. The verdict is a
Policy Kernel decision; execution is the Capability Runtime; AI is never in the
authority path.

## 12. Evidence records

S16.6 adds these record types (append-only, per S3.1):

```text
SBOM_INGESTED
PROVENANCE_VERIFIED
VEX_STATEMENT_RECORDED
REPRODUCIBLE_BUILD_VERIFIED
SUPPLY_CHAIN_RISK_SCORED
SUPPLY_CHAIN_VERDICT_EMITTED
SUPPLY_CHAIN_CLAIM_REJECTED
```

Minimum fields for `PROVENANCE_VERIFIED`:

```text
artifact_ref
subject_digest
predicate_type
builder_id
slsa_build_level
trust_root_id
subject_digest_matches
signature_valid
security_profile
verified_at
evidence_receipt_id
```

Minimum fields for `SUPPLY_CHAIN_RISK_SCORED`:

```text
artifact_ref
profile_id
sbom_completeness
provenance_level
signature_state
repro_status
open_exploitable_vulns
risk_band
verdict
blocked_reason
evidence_receipt_id
```

`SUPPLY_CHAIN_CLAIM_REJECTED` is emitted on any digest mismatch, unsigned-but-
claimed signature, AI-authored record, or unknown enum value, and feeds the S11
capability-lie / deplatforming signal.

## 13. Non-goals

- Do not invent a third SBOM format. Accept SPDX and CycloneDX; normalize, do not fork.
- Do not redefine `PackagePassport` — only its `supply_chain` block (S21 owns the rest).
- Do not treat presence of an SBOM as proof; it must normalize, correlate, and gate.
- Do not let VEX suppress a vulnerability without a signed, authorized issuer.
- Do not perform online supply-chain lookups during `AIRGAP_HIGH` admission.
- Do not claim CMVP/FIPS validation of the signing crypto here (S16.5 overlay owns that).
- Do not let AI author, sign, or approve any supply-chain artifact.
- Do not block routine `DEV_RELAXED` work for missing metadata; warn and record instead.

## 14. Acceptance criteria

S16.6 is `REAL` only when:

1. The SBOM normalizer ingests both an SPDX and a CycloneDX document into one
   `AiosSbom` and rejects unknown `SbomFormat`/`SbomComponentType`/
   `SbomRelationshipKind` values.
2. A `ProvenanceAttestation` verifies against an S11.1 trust root, and a
   subject-digest mismatch is hard-rejected and recorded as a claim rejection.
3. A signed `VexStatement` with `status = NOT_AFFECTED` and a valid justification
   relieves its CVE; an unsigned or unauthorized one does not.
4. Conditional VEX field rules are enforced (justification required for
   `NOT_AFFECTED`; action required for `AFFECTED`).
5. A `ReproducibleBuildReceipt` with a matching rebuilt digest verifies; a
   mismatch is `NOT_REPRODUCIBLE` and does not satisfy the repro gate.
6. The scanner produces a `SupplyChainRiskScore` with a profile-bound verdict and
   emits one `HARDENING_CHECK_RESULT` for control `AIOS-SR-0001` per artifact.
7. Under `STIG_ALIGNED`, an artifact lacking SBOM or verified provenance is
   blocked from admission; under `AIRGAP_HIGH`, an artifact requiring an online
   lookup at admission is blocked.
8. The computed `PackagePassport.supply_chain` block round-trips and is readable
   by the S16.3 scanner without re-parsing the original SBOM document.
9. No AI subject can author, sign, approve, or override any SBOM, provenance,
   VEX, or receipt; attempts emit `SUPPLY_CHAIN_CLAIM_REJECTED`.
10. Every verdict and rejection emits append-only evidence per S3.1.

## 15. See also

- [S16.1 Security Profile Matrix](01_security_profile_matrix.md)
- [S16.3 STIG/NIST Control Map + Scanner](03_stig_nist_control_map_scanner.md)
- [S16 Security Hardening and Compliance overview](00_overview.md)
- [S12.2 Package Model](../../002.AI-OS.NET--SPECREV.2/L6_Apps_Packages_Compatibility/02_package_model.md)
- [S11.1 Repository Model (trust roots)](../../002.AI-OS.NET--SPECREV.2/L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [S3.1 Evidence Log](../../002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/01_evidence_log.md)
- [S19 Driver and Firmware Capsule Plane](../S19_Driver_Firmware_Capsule_Plane/00_overview.md)
