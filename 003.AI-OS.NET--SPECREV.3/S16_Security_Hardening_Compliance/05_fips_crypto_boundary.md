# S16.5 — FIPS / Crypto Boundary Profile

| Field     | Value                                                                                                                                                                                                                                                         |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-29; awaiting implementation evidence)                                                                                                                                                                                             |
| Phase tag | S16.5                                                                                                                                                                                                                                                         |
| Layer     | Cross-cutting: L4 Policy/Identity/Vault, L9 Observability/Admin, L10 Distribution/Ecosystem                                                                                                                                                                   |
| Consumes  | S16.1 Security Profile Matrix, S16.4 Measured Boot, S3.1 Evidence Log                                                                                                                                                                                         |
| Produces  | `FIPS_STRICT` overlay activation contract, `ComplianceSensitiveOperation` enum, `CryptoModuleInventory`, `CryptoBoundarySelection`, `CRYPTO_BOUNDARY_SELECTED` authoritative schema, FIPS evidence records, enforcement of `hd.s16.fips_claim_without_module` |

## 1. Responsibility

S16.5 defines the **`FIPS_STRICT` crypto boundary overlay**: the contract under
which AIOS routes a closed, named set of compliance-sensitive cryptographic
operations through a **CMVP-validated cryptographic module** (FIPS 140-3), and
records the algorithm identifier and module certificate identifier as evidence.

S16.5 is the single place that answers four questions:

1. When is the `FIPS_STRICT` overlay allowed to activate, and what must be true
   first?
2. Which exact operations are _compliance-sensitive_ and therefore must use a
   validated module when the overlay is on?
3. How is each cryptographic operation bound to a specific algorithm + module
   certificate (`CryptoBoundarySelection`), so that an auditor can trace a
   claim to a certificate?
4. How does AIOS keep using BLAKE3 as a content-addressing hash (the Evidence
   Log hash chain, AIOS-FS object IDs) without breaking the FIPS boundary —
   namely, by carrying **parallel SHA-256 / SHA-384 evidence fields** where the
   overlay requires a validated digest?

S16.5 does **not** redefine the four base security profiles (S16.1), the
measured-boot chain (S16.4), or the Evidence Log record model (S3.1). It is an
overlay that sits on top of `STIG_ALIGNED` or `AIRGAP_HIGH`.

Invariant links: INV-004, INV-005, INV-008, INV-012, INV-013, INV-014, INV-017.

## 2. Product principle

> Using strong algorithms is not the same as being FIPS validated.

A self-built Rust crypto stack — Ed25519, BLAKE3, AES-GCM, HMAC-SHA256,
HKDF-SHA256, X25519 — is cryptographically strong but is **not** a validated
cryptographic module. FIPS 140-3 validates _modules_, not _algorithms_. The
product principle is therefore explicit and unforgiving:

```text
FIPS_STRICT overlay ON
  -> for every compliance-sensitive operation
       -> select a CMVP-validated module
       -> select a FIPS-approved algorithm inside that module's certificate scope
       -> run the module's power-on self-tests (KAT) and conditional self-tests
       -> record (algorithm_id, module_cert_id, provider, fips_mode) as evidence
       -> if no validated module covers the operation -> BLOCK, never silently fall back
```

The operator promise: when the overlay is on, AIOS can show an auditor _which
certificate_ covered _which operation_, and can prove that no
compliance-sensitive operation ran on a non-validated path. When AIOS cannot
prove that, it refuses to claim FIPS posture — the hard deny
`hd.s16.fips_claim_without_module` (declared in S16.1 §6) is enforced here.

## 3. Reference patterns

| Pattern                                                                                                                          | S16.5 use                                                                                                                              |
| -------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| [FIPS 140-3 / CMVP](https://csrc.nist.gov/projects/cryptographic-module-validation-program/fips-140-3-standards)                 | Validated cryptographic module standard; the certificate number is the trust anchor recorded in evidence.                              |
| [CMVP validated modules search](https://csrc.nist.gov/projects/cryptographic-module-validation-program/validated-modules/search) | Source of the `module_cert_id` namespace; an entry in `CryptoModuleInventory` must reference a real certificate id.                    |
| [FIPS 180-4 Secure Hash Standard](https://csrc.nist.gov/pubs/fips/180-4/upd1/final)                                              | SHA-256/SHA-384 are FIPS-approved digests used as the parallel evidence digest when BLAKE3 is not permitted in the validated boundary. |
| [FIPS 186-5 Digital Signature Standard](https://csrc.nist.gov/pubs/fips/186-5/final)                                             | ECDSA P-256/P-384 and Ed25519 are FIPS-approved signature algorithms; raw non-validated Ed25519 crates are not in the boundary.        |
| [SP 800-90A/B/C DRBGs](https://csrc.nist.gov/pubs/sp/800/90/a/r1/final)                                                          | Approved random bit generation for key generation inside the boundary.                                                                 |
| [SP 800-131A Rev.2 transitions](https://csrc.nist.gov/pubs/sp/800/131/a/r2/final)                                                | Algorithm/key-length acceptability used to populate the algorithm allow/deny tables.                                                   |
| [SP 800-140 series (ISO/IEC 19790 mapping)](https://csrc.nist.gov/pubs/sp/800/140/final)                                         | Module self-test and operational requirements that gate `FIPS_MODULE_LOADED`.                                                          |

## 4. FIPS_STRICT overlay — activation contract

`FIPS_STRICT` is an **overlay**, not a standalone `SecurityProfile` (S16.1 §2).
It cannot be the base profile and cannot be activated on a weak base.

### 4.1 Activation enum

```text
FipsOverlayState =
  FIPS_OFF
| FIPS_PENDING_SELFTEST
| FIPS_ACTIVE
| FIPS_DEGRADED
| FIPS_BLOCKED
```

Unknown values are rejected by the `SecurityProfileManifest` loader (the same
loader that validates S16.1 dimensions).

### 4.2 Activation preconditions (all must hold)

```text
overlay activation allowed only if:
  base_profile in { STIG_ALIGNED, AIRGAP_HIGH }          # S16.1
  and S16.4 measured-boot posture == VERIFIED             # boot chain trusted
  and CryptoModuleInventory has >= 1 ACTIVE validated module
  and every ComplianceSensitiveOperation (closed list, §5)
        is covered by at least one ACTIVE module's certificate scope
  and module power-on self-tests (KAT) PASS for each loaded module
  and operator approval is present                         # human, never AI
```

If any precondition fails, the overlay resolves to `FIPS_BLOCKED` and AIOS must
not claim FIPS posture. There is **no partial FIPS**: an operation in the closed
list that lacks validated coverage blocks activation rather than running on a
non-validated path.

### 4.3 Activation FSM

```text
FIPS_OFF
  --(operator requests overlay; preconditions §4.2 evaluated)-->
FIPS_PENDING_SELFTEST
  --(every loaded module KAT PASS + every op covered)--> FIPS_ACTIVE
  --(a required module fails self-test OR coverage gap)--> FIPS_BLOCKED
FIPS_ACTIVE
  --(module continuous/conditional self-test fails at runtime)--> FIPS_DEGRADED
  --(operator-approved disable with downgrade evidence)--> FIPS_OFF
FIPS_DEGRADED
  --(failed compliance-sensitive op forced -> hard fail)--> FIPS_BLOCKED
  --(module reloaded + self-tests PASS)--> FIPS_ACTIVE
FIPS_BLOCKED
  --(inventory repaired + self-tests PASS + re-approval)--> FIPS_PENDING_SELFTEST
```

`FIPS_DEGRADED` means a module reported a self-test fault while active: AIOS must
fail-closed for compliance-sensitive operations (block them) while leaving the
base profile's non-FIPS paths usable, and must emit `FIPS_SELFTEST_RESULT` with
`status = FAIL`. AIOS never silently demotes `FIPS_ACTIVE` to a non-validated
algorithm to "keep working".

Manifest binding to S16.1 §4 (`crypto` block):

```yaml
crypto:
  provider: validated # was "default"; "validated" selects the FIPS provider
  require_fips_validated_module: true
  parallel_sha256_for_fips_evidence: true # see §8
  fips_overlay_state: FIPS_ACTIVE
```

## 5. Compliance-sensitive operations (closed list)

When the overlay is `FIPS_ACTIVE`, exactly these operation classes MUST run
through a CMVP-validated module. The list is **closed**: an operation not in this
list is _not_ compliance-sensitive and may use the standard AIOS provider; an
operation in this list that has no validated coverage forces `FIPS_BLOCKED`
(§4.2). Unknown values are rejected by the crypto-boundary validator.

```text
ComplianceSensitiveOperation =
  EVIDENCE_SEGMENT_SIGNING          # S3.1 per-segment signatures
| EVIDENCE_RECEIPT_DIGEST           # the validated digest of a sealed record (see §8)
| POLICY_BUNDLE_SIGNATURE_VERIFY    # S2.3 policy bundle signature check
| PROFILE_BUNDLE_SIGNATURE_VERIFY   # S16.1 signed profile manifest verification
| KERNEL_MODULE_SIGNATURE_VERIFY    # S16.1 signed-module enforcement / S19 driver capsule
| PACKAGE_SIGNATURE_VERIFY          # S12.2 package + S16.x SBOM/provenance signature
| MEASURED_BOOT_QUOTE_VERIFY        # S16.4 TPM quote signature verification
| VAULT_SECRET_WRAP                 # L4 Vault Broker secret wrapping / unwrapping
| VAULT_KEY_DERIVATION              # KDF for vault-managed keys
| DATA_AT_REST_KEY_DERIVATION       # disk / dm-crypt key derivation
| DATA_AT_REST_ENCRYPTION           # symmetric encryption of persisted material
| NETWORK_TLS_HANDSHAKE             # outbound/inbound TLS in airgap export & fleet transport
| RTBF_CRYPTO_SHRED_KEY_OPS         # S16.9 per-subject key generation/destruction (INV-027)
| RANDOM_KEY_GENERATION             # DRBG-backed key generation for any of the above
```

Notably **out of scope** (non-compliance-sensitive, may stay on BLAKE3 / standard
provider when the profile permits, per §8): AIOS-FS content-address object IDs,
the Evidence Log hash-chain link digest (`previous_receipt_hash`), cache keys,
deduplication fingerprints, and non-security telemetry hashing.

## 6. Crypto module inventory

`CryptoModuleInventory` is the signed registry of cryptographic modules AIOS may
bind operations to. Only `ACTIVE` validated modules satisfy §4.2.

```yaml
crypto_module_inventory:
  inventory_id: "cminv_<ULID>"
  generated_at: "2026-05-29T00:00:00Z"
  modules:
    - module_ref: "cmod_<ULID>"
      provider: AIOS_VALIDATED_PROVIDER # see CryptoProvider enum §7.1
      module_name: "example validated module"
      module_version: "x.y.z"
      module_cert_id: "CMVP-4XXXX" # real CMVP certificate number
      cert_status: ACTIVE # ACTIVE | HISTORICAL | REVOKED | SUNSET
      validation_level: "FIPS 140-3 L1"
      approved_algorithms: # algorithm_id values inside cert scope
        - "SHA2-256"
        - "SHA2-384"
        - "AES-256-GCM"
        - "HMAC-SHA2-256"
        - "ECDSA-P256"
        - "ECDSA-P384"
        - "Ed25519"
        - "HKDF-SHA2-256"
        - "DRBG-CTR-AES256"
      covers_operations: # ComplianceSensitiveOperation values
        - EVIDENCE_SEGMENT_SIGNING
        - EVIDENCE_RECEIPT_DIGEST
        - VAULT_SECRET_WRAP
      selftest_required: true
      state: ACTIVE # ModuleState enum §7.2
```

`cert_status` and `state` are independent: `cert_status` reflects the CMVP
listing; `state` reflects the running module. A module with `cert_status =
SUNSET` or `REVOKED` MUST NOT be `state = ACTIVE`; the loader rejects that
combination.

## 7. Closed enums

### 7.1 CryptoProvider

```text
CryptoProvider =
  AIOS_DEFAULT_PROVIDER       # standard Rust crypto stack; NOT a validated module
| AIOS_VALIDATED_PROVIDER     # AIOS-packaged CMVP-validated module
| HOST_OS_VALIDATED_PROVIDER  # host-provided validated module (e.g. distro FIPS module)
| HSM_VALIDATED_PROVIDER      # hardware security module / TPM-backed validated path
| NONE                        # no provider bound (only legal when overlay is FIPS_OFF)
```

Unknown values are rejected by the crypto-boundary validator.
`AIOS_DEFAULT_PROVIDER` and `NONE` can **never** satisfy a
`ComplianceSensitiveOperation` while the overlay is `FIPS_ACTIVE`.

### 7.2 ModuleState

```text
ModuleState =
  ABSENT
| LOADED_UNTESTED
| ACTIVE
| SELFTEST_FAILED
| SUNSET
| REVOKED
```

Unknown values are rejected by the loader. Only `ACTIVE` counts toward §4.2
coverage.

### 7.3 FipsMode

```text
FipsMode =
  NON_FIPS          # operation ran on AIOS_DEFAULT_PROVIDER (overlay off / non-sensitive op)
| FIPS_APPROVED     # operation ran on a validated module with an approved algorithm
| FIPS_ALLOWED_LEGACY  # validated module, legacy-but-still-acceptable per SP 800-131A
```

Unknown values are rejected by the crypto-boundary validator. A
`ComplianceSensitiveOperation` recorded with `fips_mode = NON_FIPS` while the
overlay is `FIPS_ACTIVE` is a contract violation and MUST instead have been
blocked (§5).

## 8. BLAKE3-vs-FIPS policy

AIOS uses **BLAKE3** as its native content-addressing hash: AIOS-FS object IDs
and the Evidence Log link digest `previous_receipt_hash` are BLAKE3 (S3.1 §3).
BLAKE3 is **not** a FIPS-approved digest. The boundary is reconciled as follows:

```text
Rule B1 (content-addressing): BLAKE3 is permitted for content addressing and
  hash-chain linkage in EVERY profile, including FIPS_ACTIVE, because these
  operations are NOT in the ComplianceSensitiveOperation list (§5). Content
  addressing is an integrity/identity mechanism, not a compliance assertion.

Rule B2 (parallel evidence digest): when the manifest sets
  crypto.parallel_sha256_for_fips_evidence = true (the FIPS_ACTIVE default),
  every sealed evidence record additionally carries a FIPS-approved digest
  (SHA-256, or SHA-384 for AIRGAP_HIGH) computed by a validated module. This is
  the EVIDENCE_RECEIPT_DIGEST operation. The auditor verifies the record using
  the SHA-2 field; the BLAKE3 link digest still chains the log.

Rule B3 (no silent substitution): AIOS never replaces BLAKE3 with SHA-2 in the
  hash chain, and never replaces SHA-2 with BLAKE3 in a compliance-sensitive
  operation. Both digests coexist; neither is dropped.

Rule B4 (signature input): EVIDENCE_SEGMENT_SIGNING signs over the SHA-2
  evidence digest (B2) when the overlay is FIPS_ACTIVE, so the signature is
  rooted entirely in validated primitives.
```

Evidence record digest fields under `FIPS_ACTIVE`:

```text
link_digest_blake3   : hex_lower(BLAKE3(...))[:32]   # always present (chain)
evidence_digest_fips : { algorithm_id: "SHA2-256"|"SHA2-384",
                         module_cert_id: "CMVP-4XXXX",
                         digest_hex: "..." }          # present when B2 active
```

## 9. CryptoBoundarySelection schema

`CryptoBoundarySelection` is the per-operation record that binds one
cryptographic operation to one algorithm in one module. It is produced by the
crypto-boundary selector before the operation runs and is the payload that
`CRYPTO_BOUNDARY_SELECTED` carries.

```yaml
crypto_boundary_selection:
  selection_id: "cbsel_<ULID>"
  operation: EVIDENCE_SEGMENT_SIGNING # ComplianceSensitiveOperation (§5)
  algorithm_id: "ECDSA-P384" # must be in module.approved_algorithms
  module_cert_id: "CMVP-4XXXX" # the validated module's CMVP cert number
  module_ref: "cmod_<ULID>" # CryptoModuleInventory entry
  provider: AIOS_VALIDATED_PROVIDER # CryptoProvider (§7.1)
  fips_mode: FIPS_APPROVED # FipsMode (§7.3)
  base_profile: STIG_ALIGNED # SecurityProfile (S16.1)
  fips_overlay_state: FIPS_ACTIVE # FipsOverlayState (§4.1)
  selftest_receipt_id: "evr_..." # FIPS_SELFTEST_RESULT this binding relies on
  selected_at: "2026-05-29T00:00:00Z"
  evidence_receipt_id: "evr_..." # CRYPTO_BOUNDARY_SELECTED receipt
```

Validator rules (rejection is hard failure, not a warning):

- `algorithm_id` MUST appear in the referenced module's `approved_algorithms`.
- `module_cert_id` MUST match the referenced `module_ref`'s certificate id.
- if `fips_overlay_state = FIPS_ACTIVE` and `operation` is in §5, then
  `provider` MUST be a validated provider and `fips_mode` MUST be `FIPS_APPROVED`
  or `FIPS_ALLOWED_LEGACY`; `NON_FIPS` is rejected.
- if `fips_overlay_state = FIPS_OFF`, a `CryptoBoundarySelection` may carry
  `provider = AIOS_DEFAULT_PROVIDER`, `fips_mode = NON_FIPS`, and an empty
  `module_cert_id` — this records the truthful non-FIPS posture.

## 10. CRYPTO_BOUNDARY_SELECTED — authoritative schema

`CRYPTO_BOUNDARY_SELECTED` is **declared** in S16.1 §7 and **cross-referenced
from S16.4** (the measured-boot chain records which crypto boundary verified its
quote). S16.5 is its authoritative home. The record wraps a
`CryptoBoundarySelection` (§9) plus the chain-linkage fields every Evidence Log
record carries (S3.1 §3).

```text
CRYPTO_BOUNDARY_SELECTED  (authoritative minimum fields)

  receipt_id              # evr_<ULID>            (S3.1)
  recorded_at             # server wall-clock     (S3.1)
  subject                 # canonical L4 subject that requested the operation
  correlation_id          # ties selection -> operation -> verification
  selection_id            # cbsel_<ULID>          (§9)
  operation               # ComplianceSensitiveOperation (§5)
  algorithm_id            # FIPS-approved algorithm identifier
  module_cert_id          # CMVP certificate number  (the auditable trust anchor)
  module_ref              # cmod_<ULID>           (CryptoModuleInventory §6)
  provider                # CryptoProvider        (§7.1)
  fips_mode               # FipsMode              (§7.3)
  base_profile            # SecurityProfile       (S16.1)
  fips_overlay_state      # FipsOverlayState      (§4.1)
  selftest_receipt_id     # FIPS_SELFTEST_RESULT this selection depends on
  link_digest_blake3      # hash-chain link        (S3.1; always BLAKE3, §8 B1)
  evidence_digest_fips    # { algorithm_id, module_cert_id, digest_hex } when §8 B2 active
```

A consumer (auditor export, S16.4 measured-boot verifier, fleet evidence DAG)
that sees `fips_overlay_state = FIPS_ACTIVE` with `fips_mode = NON_FIPS` on a §5
operation MUST treat the bundle as **non-compliant** and surface
`hd.s16.fips_claim_without_module` (§12).

## 11. Security profile gates

| Profile / overlay | FIPS rule                                                                                                                                                                                        |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `DEV_RELAXED`     | Overlay forbidden. Crypto runs on `AIOS_DEFAULT_PROVIDER`; no FIPS claim may be emitted. Activation request resolves to `FIPS_BLOCKED`.                                                          |
| `SECURE_DEFAULT`  | Overlay forbidden (base too weak per §4.2). A host may _inventory_ validated modules but may not claim `FIPS_ACTIVE`.                                                                            |
| `STIG_ALIGNED`    | Overlay permitted. Requires S16.4 posture `VERIFIED`, ≥1 `ACTIVE` validated module covering every §5 operation, KAT pass, and human approval. SHA-256 parallel evidence digest required (§8 B2). |
| `AIRGAP_HIGH`     | Overlay permitted, validated module must be from a signed local mirror (no live CMVP fetch). SHA-384 parallel evidence digest required. No live algorithm/module substitution.                   |

Hard denies enforced here (in addition to S16.1 §6):

- No `FIPS_ACTIVE` claim without an `ACTIVE` validated module covering the
  operation — `hd.s16.fips_claim_without_module` (§12).
- No AI subject (`AI_NATIVE_SUBJECT`, `AI_AGENT_CAPSULE`) may activate, disable,
  or downgrade the overlay, edit `CryptoModuleInventory`, or approve a
  `CryptoBoundarySelection`. AI proposes; a `HUMAN_OPERATOR` decides.
- No silent fallback from a validated module to `AIOS_DEFAULT_PROVIDER` for a §5
  operation; the operation blocks instead.
- No downgrade of `parallel_sha256_for_fips_evidence` to `false` while the
  overlay is `FIPS_ACTIVE`.

## 12. Hard-deny enforcement: hd.s16.fips_claim_without_module

The hard deny is **declared** in S16.1 §6 ("Claim FIPS posture without validated
module evidence"). S16.5 defines its **enforcement semantics**.

```text
hd.s16.fips_claim_without_module FIRES when ANY of the following is true while a
FIPS posture is being asserted (manifest fips_overlay_state == FIPS_ACTIVE, or an
export / UI / record claims "FIPS", "FIPS-strict", or "validated crypto"):

  1. No CryptoModuleInventory module has state == ACTIVE; OR
  2. A ComplianceSensitiveOperation in §5 has no covering ACTIVE module; OR
  3. A CryptoBoundarySelection for a §5 operation has
       provider == AIOS_DEFAULT_PROVIDER  OR  fips_mode == NON_FIPS
       OR an empty/unknown module_cert_id; OR
  4. The latest FIPS_SELFTEST_RESULT for a relied-upon module is FAIL or missing; OR
  5. module_cert_id does not resolve to a real CMVP certificate in the inventory.

Effect: the Policy Kernel (S2.3) denies the asserting action, the overlay is
forced to FIPS_BLOCKED, a NON_FIPS_ALGORITHM_BLOCKED record is emitted, and the
FIPS claim is stripped from any export bundle. The host may continue operating
under its base profile (STIG_ALIGNED / AIRGAP_HIGH) — it simply may not claim
FIPS.
```

This deny is **fail-closed**: in any ambiguity (missing self-test, unresolved
certificate, partial coverage) the claim is refused. Truthful non-FIPS operation
is always preferred over an unprovable FIPS claim.

## 13. Evidence records

S16.5 adds these evidence record types (UPPER_SNAKE_CASE; the S3.1 record-type
enum is treated as frozen and these are carried in the Rev.3 evidence-delta per
DEC-R3-009):

```text
FIPS_MODULE_LOADED
CRYPTO_BOUNDARY_SELECTED
FIPS_SELFTEST_RESULT
NON_FIPS_ALGORITHM_BLOCKED
```

`FIPS_MODULE_LOADED` minimum fields:

```text
receipt_id
recorded_at
module_ref               # cmod_<ULID>  (CryptoModuleInventory §6)
module_cert_id           # CMVP certificate number
provider                 # CryptoProvider (§7.1)
module_version
cert_status              # ACTIVE | HISTORICAL | REVOKED | SUNSET
validation_level         # e.g. "FIPS 140-3 L1"
covers_operations        # list of ComplianceSensitiveOperation (§5)
loaded_state             # ModuleState (§7.2)
base_profile             # SecurityProfile (S16.1)
fips_overlay_state       # FipsOverlayState (§4.1)
evidence_receipt_id
```

`FIPS_SELFTEST_RESULT` minimum fields:

```text
module_ref
module_cert_id
selftest_kind            # POWER_ON_KAT | CONDITIONAL | CONTINUOUS
algorithm_id
status                   # PASS | FAIL
observed_redacted
evidence_receipt_id
```

`NON_FIPS_ALGORITHM_BLOCKED` records the §12 deny: the requested operation,
the attempted provider/algorithm, the failing precondition, and the denying
policy id `hd.s16.fips_claim_without_module`.

## 14. Non-goals

- Do not claim "FIPS validated", "FIPS certified", or "CMVP listed" for AIOS as
  a whole. AIOS _binds operations to_ a validated module; it is not itself the
  validated module.
- Do not treat strong algorithms (Ed25519, BLAKE3, AES-GCM via standard crates)
  as FIPS validation. Using a strong algorithm is not the same as using a
  validated module — this is the entire point of S16.5.
- Do not make BLAKE3 illegal: it stays the content-addressing and hash-chain
  primitive in every profile (§8 B1).
- Do not allow a "best effort" or "partial" FIPS posture. The overlay is binary
  per operation: covered-and-validated, or blocked.
- Do not let the overlay weaken the base profile; it only adds constraints.
- Do not let AI activate, manage, or vouch for the crypto boundary.
- Do not fetch validated modules over the live network under `AIRGAP_HIGH`.

## 15. Acceptance criteria

S16.5 is `REAL` only when:

1. `FipsOverlayState`, `ComplianceSensitiveOperation`, `CryptoProvider`,
   `ModuleState`, and `FipsMode` enums parse and reject unknown values.
2. The overlay refuses to activate (`FIPS_BLOCKED`) on `DEV_RELAXED` or
   `SECURE_DEFAULT`, and on any host where S16.4 posture is not `VERIFIED`.
3. The overlay refuses to activate when any §5 operation lacks an `ACTIVE`
   validated module covering it — proven by a coverage-gap test.
4. A `CryptoBoundarySelection` binds `algorithm_id` + `module_cert_id` +
   `provider` + `fips_mode`, and the validator rejects a §5 operation recorded
   with `AIOS_DEFAULT_PROVIDER` or `fips_mode = NON_FIPS` while `FIPS_ACTIVE`.
5. Every compliance-sensitive operation under `FIPS_ACTIVE` emits a
   `CRYPTO_BOUNDARY_SELECTED` record carrying the CMVP certificate id.
6. BLAKE3 remains the hash-chain link digest while a parallel SHA-256
   (`STIG_ALIGNED`) or SHA-384 (`AIRGAP_HIGH`) FIPS evidence digest is produced
   by a validated module (§8 B2), and neither digest is dropped.
7. `hd.s16.fips_claim_without_module` fires on each of the five §12 conditions,
   forces `FIPS_BLOCKED`, emits `NON_FIPS_ALGORITHM_BLOCKED`, and strips the FIPS
   claim from exports — while leaving base-profile operation intact.
8. A module self-test `FAIL` moves `FIPS_ACTIVE -> FIPS_DEGRADED`, blocks §5
   operations, and emits `FIPS_SELFTEST_RESULT` with `status = FAIL`.
9. No AI subject can activate, disable, downgrade, or approve any part of the
   crypto boundary; such attempts are denied and recorded.
10. Recovery and audit export can show the active overlay state, the bound
    module certificate ids, and the per-operation `fips_mode` without invoking
    the Cognitive Core.

## 16. See also

- [S16.1 Security Profile Matrix](01_security_profile_matrix.md)
- [S16.4 Measured Boot](04_measured_boot.md)
- [S16 Security Hardening and Compliance](00_overview.md)
- [S3.1 Evidence Log](../../002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/01_evidence_log.md)
- [Rev.3 Planning Notes](../00_PLANNING_NOTES.md)
- [Rev.3 Design Decisions](../02_design_decisions.md)
