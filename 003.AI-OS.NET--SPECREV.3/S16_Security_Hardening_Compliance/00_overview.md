# S16 — Security Hardening and Compliance

| Field     | Value                                                                                                                                                  |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Status    | `CONTRACT` (created 2026-05-28; awaiting implementation evidence)                                                                                      |
| Phase tag | S16                                                                                                                                                    |
| Layer     | Cross-cutting: L0, L1, L4, L6, L8, L9, L10                                                                                                             |
| Consumes  | S2.3 Policy Kernel, S3.1 Evidence Log, S9.1 Recovery Boundary, S3.2 Sandbox Composition, S12.2 Package Model, S8.5 Firmware Trust, S8.1 Network Policy |
| Produces  | security profiles, SELinux MAC policy plane, STIG/NIST/CIS control map, scanner/export contracts                                                       |

## 1. Responsibility

S16 defines the high-assurance security baseline for AIOS Rev.3. It turns the
planning statement "STIG-aligned, SELinux-enforced, evidence-backed" into
machine-readable contracts.

S16 does not certify AIOS. It defines the technical path required before AIOS
may truthfully claim a STIG-aligned hardening profile.

Invariant links: INV-002, INV-004, INV-005, INV-008, INV-012, INV-013, INV-014,
INV-015, INV-017, INV-018.

## 2. Compliance boundary

Allowed language once implemented:

- `STIG-aligned hardening profile`
- `NIST 800-53 mapped controls`
- `CIS-mapped community hardening profile`
- `FIPS-strict crypto boundary profile` only when using a validated module and
  recording module certificate evidence

Forbidden language until formal assessment exists:

- `DoD certified`
- `military certified`
- `STIG compliant`
- `FIPS validated`
- `Common Criteria certified`

## 3. External baselines

| Baseline                                                                                                         | S16 use                                                                                             |
| ---------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| [DISA STIGs](https://public.cyber.mil/stigs/)                                                                    | Authoritative hardening target and checklist source.                                                |
| [NIST SP 800-53 Rev. 5](https://csrc.nist.gov/pubs/sp/800/53/r5/upd1/final)                                      | Control catalog and control family map; current NIST page notes Release 5.2.0 updates.              |
| [NIST SP 800-218 SSDF](https://csrc.nist.gov/pubs/sp/800/218/final)                                              | Secure development/release rules for AIOS packages, adapters, policy bundles, and kernel artifacts. |
| [NIST SP 800-207 Zero Trust](https://csrc.nist.gov/pubs/sp/800/207/final)                                        | Fleet/network trust model: no implicit LAN trust.                                                   |
| [NIST SP 800-193 Firmware Resiliency](https://csrc.nist.gov/pubs/sp/800/193/final)                               | Firmware protection, detection, recovery contract.                                                  |
| [FIPS 140-3 / CMVP](https://csrc.nist.gov/projects/cryptographic-module-validation-program/fips-140-3-standards) | Optional strict crypto boundary using validated modules.                                            |
| [CIS Controls v8](https://www.cisecurity.org/controls/v8)                                                        | Practical community/operator control grouping.                                                      |
| [OpenSCAP](https://static.open-scap.org/openscap-1.3/oscap_user_manual.html)                                     | Scanner/export compatibility target for XCCDF/ARF-style artifacts where practical.                  |

## 4. Sub-specs

| File                                    | Topic                                                                                                                                               | Status     | Priority |
| --------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- | -------- |
| `01_security_profile_matrix.md`         | `SecurityProfile` enum, hardening posture, transition gates, evidence records.                                                                      | `CONTRACT` | P0       |
| `02_selinux_mac_policy_plane.md`        | SELinux domains, labels, MCS/MLS mapping, policy bundle lifecycle, AVC evidence.                                                                    | `CONTRACT` | P0       |
| `03_stig_nist_control_map_scanner.md`   | Control map schema, scanner pipeline, evidence, exception register, export formats.                                                                 | `CONTRACT` | P0       |
| `04_measured_boot_runtime_integrity.md` | Dual-chain root of trust: TPM 2.0 measured boot, signed quotes, IMA appraisal, remote attestation, boot/runtime integrity evidence.                 | `CONTRACT` | P0       |
| `05_fips_crypto_boundary.md`            | `FIPS_STRICT` overlay: CMVP-validated module routing, compliance-sensitive operation enum, crypto-boundary selection, parallel SHA evidence fields. | `CONTRACT` | P1       |
| `06_sbom_provenance_vex.md`             | Supply-chain evidence: SBOM, SLSA-style provenance, signed VEX, reproducible-build receipts, `PackagePassport.supply_chain` schema.                 | `CONTRACT` | P1       |
| `07_service_hardening_score_gates.md`   | `ServiceClass` enum, systemd unit hardening requirements, `ServiceHardeningScore`, per-class score floors, service promotion gate.                  | `CONTRACT` | P1       |
| `08_zero_trust_fleet_posture.md`        | NIST 800-207 fleet posture: `ZeroTrustPosture`, device trust state, per-request authorization, continuous posture re-evaluation.                    | `CONTRACT` | P2       |
| `09_data_governance_gdpr.md`            | Personal-data classification, crypto-shred RTBF erasure (INV-027), data residency, audit export bundles (SOC 2 / ISO 27001 / HIPAA).                | `CONTRACT` | P2       |

## 5. Cross-cutting invariants

1. Security profile state is explicit and evidence-backed.
2. SELinux is the primary MAC backend for `STIG_ALIGNED` and `AIRGAP_HIGH`.
3. Policy, evidence, vault, recovery, and agent domains must never run as
   `unconfined_t`.
4. Compliance exceptions are records, not comments.
5. Failed hardening checks block promotion to `STIG_ALIGNED`.
6. AI subjects cannot approve hardening exceptions, SELinux relaxations, FIPS
   downgrades, boot-chain changes, or audit-retention reductions.
7. Evidence is append-only and exportable into auditor-friendly formats.

## 6. Minimum implementation artifacts

Before S16 can be marked `REAL`, AIOS must provide:

- a `SecurityProfileManifest` parser and validator
- a signed default profile set
- SELinux policy module packaging for AIOS domains
- boot/runtime posture probes (measured boot, TPM quotes, IMA appraisal per
  [§S16.4](04_measured_boot_runtime_integrity.md))
- `FIPS_STRICT` crypto-boundary enforcement and module inventory per
  [§S16.5](05_fips_crypto_boundary.md)
- SBOM, provenance, and VEX generation/verification per
  [§S16.6](06_sbom_provenance_vex.md)
- service hardening scorer and promotion gate per
  [§S16.7](07_service_hardening_score_gates.md)
- `aios-hardening-audit`
- JSON and Markdown evidence reports
- OpenSCAP/XCCDF-compatible export where practical
- CKLB/STIG Viewer export adapter if the target schema is stable enough
- exception register with expiry, owner, compensating control, and evidence

## 7. Non-goals

- S16 does not certify, accredit, or formally assess AIOS. It defines the
  technical path to a `STIG-aligned` posture; certification language stays
  forbidden until a real assessment exists (see §2).
- No S16 profile — including `STIG_ALIGNED`, `AIRGAP_HIGH`, or `FIPS_STRICT` —
  may weaken the recovery boundary (S9.1) or append-only evidence (S3.1).
  Hardening that breaks recovery or auditability is a defect, not a profile.
- S16 does not perform legal compliance attestation (GDPR/SOC 2/ISO/HIPAA). It
  produces classifications, controls, and exportable evidence; the legal
  determination is an external duty of the operator/auditor.
- S16 does not own the underlying mechanisms it constrains — the Policy Kernel
  (S2.3), sandbox composition (S3.2), firmware trust (S8.5), and network policy
  (S8.1). It maps and gates them; it does not reimplement them.
- S16 does not grant AI subjects authority over hardening state. Exceptions,
  SELinux relaxations, FIPS downgrades, and audit-retention changes are
  human-only (see §5.6).

## 8. Acceptance criteria

S16 (the plane) is `REAL` only when every sub-spec S16.1–S16.9 reaches `REAL` with its own
evidence, and additionally:

1. The four security profiles plus the `FIPS_STRICT` overlay are representable in signed manifests (S16.1, S16.5).
2. SELinux is the enforcing MAC backend for `STIG_ALIGNED`/`AIRGAP_HIGH`; no AIOS-owned service runs `unconfined_t` (S16.2, S16.7).
3. A host can produce a STIG/NIST/CIS control-map scan with an exception register and an auditor-friendly export (S16.3).
4. Measured boot (Secure Boot + TPM dual chain + IMA/EVM) gates `STIG_ALIGNED`/`AIRGAP_HIGH`, and boot failure drops to recovery with evidence, never silent permissive mode (S16.4).
5. SBOM/provenance/VEX and reproducible-build receipts are required for packages under strict profiles (S16.6).
6. GDPR erasure is satisfied by crypto-shred without breaking the append-only evidence chain (S16.9, INV-027).
7. No AI subject can approve a hardening exception, SELinux relaxation, FIPS downgrade, boot-chain change, or audit-retention reduction (INV-002, INV-028).

## 9. See also

- [Security Profile Matrix](01_security_profile_matrix.md)
- [SELinux MAC Policy Plane](02_selinux_mac_policy_plane.md)
- [STIG/NIST Control Map + Scanner](03_stig_nist_control_map_scanner.md)
- [Measured Boot and Runtime Integrity](04_measured_boot_runtime_integrity.md)
- [FIPS / Crypto Boundary Profile](05_fips_crypto_boundary.md)
- [SBOM + Provenance + VEX](06_sbom_provenance_vex.md)
- [Service Hardening Score Gates](07_service_hardening_score_gates.md)
- [Zero-Trust Fleet Posture](08_zero_trust_fleet_posture.md)
- [Data Governance, GDPR/RTBF and Audit Export](09_data_governance_gdpr.md)
- [Rev.3 Planning Notes](../00_PLANNING_NOTES.md)
