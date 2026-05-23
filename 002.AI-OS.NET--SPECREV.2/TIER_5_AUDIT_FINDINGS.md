# Tier 5 Audit + Simulation — Master Findings Report

| Field      | Value                                                                                        |
| ---------- | -------------------------------------------------------------------------------------------- |
| Date       | 2026-05-11                                                                                   |
| Phase      | Tier 5 (audit + simulation cycle 1)                                                          |
| Method     | 5 audit dimensions + 4 action-path simulations, run in parallel as 9 read-only agents        |
| Scope      | All 52 contract-grade sub-specs across L0–L10 + XX_Cross_Cutting + 4 top-level docs          |
| Discipline | READ-ONLY agents, structured findings format with severity, source citation, recommended fix |

## Executive summary

Total findings: ~93 across 9 audit/simulation reports. Critical headlines:

- **SIM-D (INV-002 bypass attempts at all 6 sites): ZERO findings.** All 6 enforcement sites have closed-enum reject codes, FOREVER (or design-justified STANDARD_24M) records, append-authority-restricted-to-non-AI-services, and hash-chain-protected emissions. The constitutional core of "AI proposes, never executes" is mechanically intact.
- \*\*One CRITICAL finding (SIM-A-003): first-boot path mutates `RecoveryMutableScope` paths without `recovery_mode = true` or HUMAN_USER approver. INV-012 hard-deny `RecoveryRequiredForSystemMutation` would fire on every first-boot system mutation. A faithful implementation cannot complete first-boot without a constitutional discriminator (4th `RecoveryMode` value `FIRST_BOOT` or `is_first_boot` flag).
- **Systemic IDL roll-up debt** (LOG-001/002/005/006/013, CONST FINDING-HIGH-004): Wave 5/6/7/8 added ~378 record types and ~13 properties to narrative catalogs without updating the proto IDL. The spec is in a documented half-state. 6 CONST findings collapse to one decision.
- **Layer dependency rule widely violated through `Consumes` declarations** (ARCH-001..006, ARCH-010, ARCH-011): L0 declares upward dep on L9; L1 on L4 and L7; L2 on L4/L5/L6/L9; L3 on L4/L9; L7 on L8. Architecture overview softens rule only for L1→L5, but base rule is broader. Either the rule must be relaxed in writing or `Consumes` reframed as "vocabulary import" vs "runtime requirement".
- **INV-002 site 2 mechanism does not fire for user-scope installs** (SIM-B-002 + ARCH-008 + SIM-B-001 + SIM-B-009): `AISystemAdminBlocked` requires `target.scope = SYSTEM`; user-scope `pkg:gimp@2.10.36` does not match; the cited alternative (S0.1 envelope FSM) has no AI-specific guard. Real enforcement is Policy Kernel REQUIRE_APPROVAL with `approver_subject_filter = HUMAN_USER` — works, but not what the spec narrative cites.

## Severity tally

| Audit              |  Total | CONST/CRIT |   HIGH |    MED |    LOW |
| ------------------ | -----: | ---------: | -----: | -----: | -----: |
| LOG (logical)      |     18 |          6 |      5 |      5 |      2 |
| ARCH               |     14 |          6 |      4 |      3 |      1 |
| CONST              |      5 |          0 |      1 |      3 |      1 |
| CONS-S31           |     15 |          0 |      0 |      5 |     10 |
| CONS-S24/S10/S41   |     12 |          0 |      0 |      6 |      6 |
| SIM-A (first boot) |     12 |          1 |      4 |      5 |      2 |
| SIM-B (AI install) |     11 |          0 |      1 |      5 |      5 |
| SIM-C (HW drift)   |      6 |          1 |      2 |      2 |      1 |
| SIM-D (INV-002)    |  **0** |          — |      — |      — |      — |
| **Total**          | **93** |     **14** | **17** | **34** | **28** |

## Root-cause clusters

The 93 findings collapse to **14 clusters** when grouped by underlying cause. Each cluster is a fix-target.

### Cluster 1 — IDL roll-up debt (CONST/HIGH severity)

**Findings:** LOG-001, LOG-002, LOG-005, LOG-006, LOG-013, FINDING-HIGH-004 (CONST audit), LOG-018.

**Root cause:** Wave 5/6/7/8 added narrative entries to S3.1 RecordType (22→400) and S2.4 PropertyType (9→22) without updating Appendix A proto IDL. Multiple consumer specs cite "added to S3.1" / "existing closed PropertyType" for entries that exist only in narrative form.

**Fix:** Single deliberate IDL roll-up sweep. Two acceptable end-states:

- (a) **IDL-as-source-of-truth**: rebuild Appendix A enums to match all narrative additions (~378 records, ~13 properties); or
- (b) **Narrative-as-source-of-truth**: relabel narrative entries as proposals; freeze IDL Appendix A as the canonical 22-entry / 9-entry boundary; require explicit IDL extension rite for each promotion.

Recommend (a). Generate proto3 enum entries with stable numeric IDs reserving id 22 for `TAMPER_DETECTED` (already there) and adding new IDs starting at 23.

### Cluster 2 — First-boot constitutional gap (CRITICAL)

**Findings:** SIM-A-003 (CRITICAL), SIM-A-004 (HIGH), SIM-A-006 (MED), SIM-A-008 (MED).

**Root cause:** S9.2 first-boot stages 5–12 mutate `RecoveryMutableScope` paths (`VAULT_ROOT_MATERIAL`, `INVARIANT_BUNDLE`, `POLICY_BUNDLE`, `IDENTITY_BUNDLE`, `RECOVERY_OPERATOR_REGISTRATION`, etc.) but the host is not in `recovery_mode = true` and there is no HUMAN_USER approver yet. INV-012 hard-deny would fire on every first-boot mutation.

**Fix:** Add 4th `RecoveryMode` enum value `FIRST_BOOT` (or `is_first_boot` boolean on subject sessions). Update S2.3 §26.2.2 hard-deny `RecoveryRequiredForSystemMutation` to allow `is_first_boot OR is_recovery_mode`. Update S5.1 §5.2 group-registration rule for first-boot exception. Update S5.2 vault for `BOOTSTRAP_KEY_SIGN` exception class. Self-extinguishing once firstboot marker written.

### Cluster 3 — Layer dependency rule violations (CONSTITUTIONAL)

**Findings:** ARCH-001..006, ARCH-010, ARCH-011 (8 findings).

**Root cause:** `Consumes` declarations in sub-spec headers list higher-numbered layers as dependencies. The rule is unconditional in `00_overview.md` and Rev.1 §6 but architecture overview softens it only for L1→L5/L7 and L0→L5/L7.

**Fix:** Add an authoritative refinement of the rule (likely in a new S0.5 or 03_architecture_overview addendum): "Each layer must boot/recover/fail-safe without higher layers operational; cross-spec **vocabulary** sharing is permitted via closed-enum imports (`shares schema with`), not via runtime requirements (`requires for correctness`)." Then audit each `Consumes` against the refined rule and reclassify.

### Cluster 4 — INV-002 site 2 mechanism mismatch (HIGH)

**Findings:** SIM-B-001, SIM-B-002 (HIGH), SIM-B-009, ARCH-008.

**Root cause:** Site 2 cited as enforced by `AISystemAdminBlocked` (S2.3 §26.2.3) and "S0.1 envelope FSM AI-install rejection". But `AISystemAdminBlocked` only fires for `target.scope = SYSTEM`; user-scope installs are not caught. S0.1 FSM has no AI-install guard. Real enforcement is upstream Policy Kernel REQUIRE_APPROVAL with `approver_subject_filter = HUMAN_USER`.

**Fix:** Add explicit constitutional hard-deny `AIInstallInitiationBlocked` to S2.3 §26 / §27 firing on `subject.is_ai = true AND request.action IN {package.install, app.install, package.uninstall.execute}`. Update S0.4 §4.2 enforcement map row 2 to cite the correct mechanism. Reconcile reject code: either rename S11.1's `PACKAGE_VERIFICATION_FAILED(reason=AI_DIRECT_INSTALL_DENIED)` to `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED`, or document Site 2 as having two mechanical sub-sites.

### Cluster 5 — Missing record types referenced in spec body (MED)

**Findings:** LOG-002, CONS-S31-001, CONS-S31-003, CONS-S31-004, CONS-S31-005, CONS-S31-006, CONS-S31-007, CONS-S31-008 (and parts of LOG-003, LOG-018).

**Root cause:** Multiple specs reference RecordTypes by name that never landed in S3.1's Wave consolidations. Includes `INVARIANT_BUNDLE_LOADED`, `IDENTITY_BUNDLE_LOADED`, `GROUP_REGISTERED`, `STATUS_TRANSITION`, `BUILD_PASSED`, `WEB_EXPOSURE_GRANTED`, `RECEIPT_FORGERY_DETECTED`, `RECEIPT_PAYLOAD_DUPLICATE_OBSERVED`, `RECEIPT_LINEAGE_DEPTH_EXCEEDED`, `RECEIPT_ORPHAN_ACTION_REF_DETECTED`, `*_BUNDLE_REJECTED` family (5), `FAILURE_OBSERVED_RATE_LIMITED`, `GRAPH_EVALUATION_BUDGET_EXCEEDED`, `TRANSITION_BUDGET_EXCEEDED`, `AB_OBSERVATION_WINDOW_ROLLBACK`, `UNIT_MANIFEST_FORGERY_DETECTED`, `ADAPTER_LIFECYCLE_ILLEGAL_TRANSITION`, `IDENTITY_BIND_FAILED`, `PUBLISHER_TRUST_LEVEL_OBSERVED`, `PUBLISHER_KEY_COLLISION`, `MARKETPLACE_REVIEW_BUDGET_EXCEEDED`, `BRIDGE_OPERATOR_CONSENT_GRANTED`, `BRIDGE_DEFERRED_NEEDS_REVIEW`, `BRIDGE_METADATA_DRIFT_DETECTED`, `BRIDGE_BLACKLIST_LIFTED` — ~25 missing record types.

**Fix:** Wave 9 micro-consolidation for the orphan record types. Add each with retention class per source spec.

### Cluster 6 — Closed-enum drift / phantom enum values (CONST/HIGH)

**Findings:** LOG-004 (`LONG` retention class not in `RetentionClass`), LOG-006 (3 phantom PropertyTypes), CONS-S24-001 (`STATUS_GRADE_CONSISTENT` phantom), CONS-S24-002 (`POLICY_AI_SELF_APPROVAL_BLOCKED` phantom), SIM-B-001 (`AI_DIRECT_INSTALL_DENIED` not in `PackageVerificationResult`), SIM-B-004 (no closed reject code in `NetworkPolicyErrorCode` for site 3).

**Fix:** Per-finding decision: either extend the enum or remove the phantom citation.

### Cluster 7 — Wave 8 incomplete (MED)

**Findings:** CONS-S10-001 (S15.1 unit lifecycle 5 actions missing from S10.1 W8), CONS-S10-002 (S15.3 adapter lifecycle 4 actions missing), CONS-S24-003 (3 namespace properties from S4.1 W8.4 not picked up by S2.4 W8).

**Fix:** Wave 9 amendment to S10.1 catalog (+9 actions) and S2.4 (+3 properties).

### Cluster 8 — `hardware.accept_drift` constitutional gap (HIGH)

**Findings:** SIM-C-001 (HIGH), CONS-S10-003 confirms.

**Root cause:** Single HUMAN_USER action conflates accessory hardware drift (laptop GPU swap) with constitutional substrate drift (CPU swap, TPM substitution, BIOS/UEFI replacement). S8.5 §9.3 already enforces RECOVERY_ONLY for firmware updates of those substrates, but the same substrates accepted via drift only require HUMAN_USER.

**Fix:** Split into `hardware.accept_drift_accessory` (HUMAN_USER) and `hardware.accept_drift_substrate` (RECOVERY_ONLY + hardware-key co-signer). Update S2.3 with closed condition field `target.is_constitutional_substrate: bool`. Promote `HARDWARE_GRAPH_DRIFT_FOREVER` to L0 (also queued) and add RECOVERY_ONLY rider for substrate subset.

### Cluster 9 — Hardware drift accept-side and TPM rekey gaps (HIGH)

**Findings:** SIM-C-002 (HIGH — no `HARDWARE_DRIFT_ACCEPTED` record), SIM-C-003 (HIGH — vault rekey on TPM swap not wired).

**Fix:** Add `HARDWARE_GRAPH_DRIFT_ACCEPTED` (FOREVER) to S8.3 §12 + S3.1. Add S5.2 producer row for `VAULT_TPM_RESEAL_REQUIRED`. Add S2.4 primitive `vault_seal_pcr_set_matches(host_id)`.

### Cluster 10 — S9.1 RecoveryMutableScope enum extensions needed (MED)

**Findings:** CONS-S41-001, CONS-3W-002 (transitive blocker).

**Root cause:** S4.1 W8.4 declares `SYS_FIRSTBOOT_RESET` and `FIRMWARE_VERSION_COUNTER` as needed S9.1 enum values; S8.5 and S9.2 also reference them; S9.1 enum still has 8 values.

**Fix:** Bump S9.1 §3.6 `RecoveryMutableScope` enum to 10 entries (add the two values).

### Cluster 11 — Cited but undefined: INV-007 in S5.4 (HIGH)

**Findings:** LOG-007.

**Root cause:** S5.4 cites `INV-007 ("Hard-deny cannot be silently overridden")` ~6 times, but INV-007 is `LAYER_DOWNWARD_DEPENDENCY`. The cited principle is not in the catalog.

**Fix:** Either add new constitutional invariant (deliberate constitutional act per DEC-025/026), or rewrite S5.4 references to point to existing invariants (likely INV-005 + INV-014 jointly).

### Cluster 12 — ID prefix discipline split (HIGH)

**Findings:** LOG-008 (`act_` vs `act:` vs `app:` vs `ovr:`), LOG-009 (`tplan_[:48]` vs `[:32]`).

**Fix:** Normalise to one separator (`_`) across all sub-specs. Assign distinct prefixes for request vs binding (mirror S5.4's `ovrq_` / `ovr_` discipline); document `tplan_` truncation as deliberate exception with rationale, or normalise to `[:32]`.

### Cluster 13 — L5 cognitive FSM positive transitions invisible (MED)

**Findings:** SIM-B-003.

**Fix:** Add `AGENT_LIFECYCLE_TRANSITIONED` (STANDARD_24M) to S13.1 §16 + S3.1. Audit chain becomes positive-witness rather than absence-witness.

### Cluster 14 — Documentation hygiene (LOW)

**Findings:** LOG-014, LOG-015, LOG-016, LOG-017, FINDING-MED-003 (CONST), CONS-S41-003 (arithmetic miscount), CONS-S31-010, CONS-S31-011 (typo `LISTING_LISTING_`), CONS-S31-012, CONS-S31-013, CONS-S31-014, CONS-S31-015 (baseline jump unexplained), and others.

**Fix:** Documentation pass — fix typos, refresh stale "to be written" markers (INV-019..023 reference S7.1/S7.3 which now exist), correct arithmetic, add synonym/adjacency notes, document `payload_digest` JCS step, etc.

## Fix-wave plan

Findings are fixable in **3 parallelisable waves**.

### Wave 9 (constitutional surgery — sequential, no parallel)

Mutually-dependent edits at the constitutional core:

1. Cluster 2 (first-boot constitutional gap) — adds `RecoveryMode.FIRST_BOOT`
2. Cluster 4 (INV-002 site 2 mechanism) — adds `AIInstallInitiationBlocked` hard-deny
3. Cluster 8 (hardware.accept_drift split)
4. Cluster 11 (INV-007 misnaming in S5.4)

These touch S2.3 (Policy Kernel) and S6.4 (invariants) which other specs cite — must be authored carefully and committed atomically.

### Wave 10 (vocabulary roll-up — parallel-safe)

Mostly additive; no constitutional structure changes:

5. Cluster 1 (IDL roll-up sweep) — single large edit to S3.1 Appendix A + S2.4 Appendix A
6. Cluster 5 (orphan record types) — Wave 9 micro-consolidation
7. Cluster 6 (phantom enum values) — per-finding fix
8. Cluster 7 (Wave 8 completion) — S10.1 +9 actions, S2.4 +3 properties
9. Cluster 9 (HW drift accept records) — S8.3 + S3.1 + S5.2 + S2.4
10. Cluster 10 (S9.1 RecoveryMutableScope +2 values)
11. Cluster 13 (L5 lifecycle transition record)

### Wave 11 (parallel-safe — documentation + architecture refinement)

12. Cluster 3 (layer dependency rule refinement) — new S0.5 or addendum + per-spec `Consumes` reclassification
13. Cluster 12 (ID prefix normalisation)
14. Cluster 14 (documentation hygiene)

After Wave 11, **re-audit** with the same 9 agents. Iterate Wave 12+ until convergence (zero CONSTITUTIONAL findings, ideally zero HIGH).

## See also

- [00_MASTER_INDEX.md](00_MASTER_INDEX.md)
- [02_design_decisions.md](02_design_decisions.md) — DEC-046 records this audit cycle; DEC-047..051 record fix waves 9–13 (cycles 1+2 closed)
- [XX_Cross_Cutting/04_constitutional_meta_principles.md](XX_Cross_Cutting/04_constitutional_meta_principles.md) — INV-002 enforcement map (clean per SIM-D)
