# Evidence Grades (Rev.2)

| Field          | Value                                                                                                              |
| -------------- | ------------------------------------------------------------------------------------------------------------------ |
| Status         | `CONTRACT` (initial; written 2026-05-09)                                                                           |
| Phase tag      | S6.2                                                                                                               |
| Layer          | L0 Governance, Evidence, Safety                                                                                    |
| Schema package | `aios.governance.v1alpha1`                                                                                         |
| Consumes       | nothing (L0 is the bottom of the dependency stack)                                                                 |
| Produces       | the canonical evidence-grade taxonomy used by S6.1 status taxonomy and consumed by every spec for promotion gating |

## 1. Purpose

`No proof = no completion` is the constitutional rule. This spec fixes what "proof" means at the system level: a closed grade vocabulary `E0..E5`, criteria for what artifacts qualify at each grade, escalation rules between grades, and the grade-to-status mapping consumed by S6.1 status taxonomy gates.

## 2. Core invariants

- **I1 — Closed grade vocabulary.** Six grades, no more. Adding a grade is a versioned spec change.
- **I2 — Grades are mechanical.** Each grade has a deterministic check function. Operator opinion ("looks fine") is not a grade.
- **I3 — Grade is per-capability, not per-artifact.** A capability accumulates evidence across many artifacts; its current grade is the maximum grade for which all required artifacts are present and current.
- **I4 — Grades degrade automatically.** When an artifact becomes stale, the affected grade drops; the capability's current grade is recomputed.
- **I5 — Higher grades subsume lower.** `E3` evidence implies `E2` evidence implies `E1`. A capability claiming `E3` must also satisfy `E2` and `E1` requirements.
- **I6 — Evidence is signed.** Every evidence receipt is signed by the producing component (per S3.1 Ed25519 segment signature). Unsigned or signature-failing receipts do not count toward grade.

## 3. The six grades

```proto
enum EvidenceGrade {
  EVIDENCE_GRADE_UNSPECIFIED = 0;
  E0 = 1;       // none
  E1 = 2;       // artifact exists
  E2 = 3;       // build / typecheck / proto-IDL compile
  E3 = 4;       // unit / integration test
  E4 = 5;       // end-to-end / recovery / release gate
  E5 = 6;       // live operational
}
```

### 3.1 `E0` — none

No evidence exists for this capability.

**When this grade applies:** the capability is a `DEFERRED` decision, an `UNKNOWN`, or a `BLOCKED` whose blocker has not yet produced evidence.

**Required artifacts:** none.

**Verification:** trivial (absence of any S3.1 receipt referencing the capability).

### 3.2 `E1` — artifact exists

A persistent artifact for the capability is recorded in AIOS-FS or in the evidence log. The artifact may be:

- A spec file (`.md`) with the capability's name and a non-empty body.
- A code file (function signature, type declaration, route placeholder).
- A configuration file declaring the capability is intended.
- A retirement record (for `RETIRED` capabilities).
- A deferral record (for `DEFERRED`).

**Required artifacts:** at least one S3.1 receipt of type `ARTIFACT_RECORDED` referencing the capability id and an AIOS-FS object pointer.

**Verification:** S2.4 verification primitive `aiosfs_pointer(capability_id)` resolves to a non-empty object.

### 3.3 `E2` — build / typecheck / compile

The capability's defining artifacts compile or typecheck successfully. For specs, this means proto IDL files referenced by the spec compile cleanly. For code, this means the package builds without errors.

**Required artifacts:**

- Build log artifact recorded as an evidence receipt.
- Receipt of type `BUILD_PASSED` with a build-system identifier (`cargo`, `rustc`, `protoc`, `tsc`, `go build`, etc.) and the exit code.

**Verification:** S2.4 verification primitive `evidence_exists(receipt_kind=BUILD_PASSED, capability_id=X, age < 7d)`.

**Staleness:** an `E2` claim is stale if the most recent `BUILD_PASSED` receipt is older than the most recent change to the capability's source artifacts.

### 3.4 `E3` — unit / integration test

The capability has at least one automated test that exercises its acceptance criteria, and that test passes.

**Required artifacts:**

- Test definition artifact (test file) — covered under E1.
- Test execution receipt of type `TEST_PASSED` with a test-runner identifier (`cargo test`, `pytest`, `playwright`, etc.), the test-name list, and the exit code.

**Verification:** S2.4 verification primitive chain:

```text
AllOf [
  evidence_exists(receipt_kind=BUILD_PASSED, capability_id=X, age < 7d),
  evidence_exists(receipt_kind=TEST_PASSED,  capability_id=X, age < 7d),
  property STATUS_GRADE_CONSISTENT  // catches inconsistencies between claimed grade and actual receipts
]
```

**Required scope:** the test must cover at least one acceptance criterion from the capability's spec. A capability with five acceptance criteria but only one tested is `E3` only with respect to that criterion; the other four remain at `E2` until tests are added.

**Staleness:** an `E3` claim is stale if the most recent `TEST_PASSED` receipt is older than the most recent change to either source or test artifacts.

### 3.5 `E4` — end-to-end / recovery / release gate

The capability has been exercised under realistic conditions:

- An end-to-end test that drives the full action flow (S0.1 envelope → S2.3 policy → S3.2 sandbox → execution → S3.1 evidence → S2.4 verification) for a representative scenario.
- Or a recovery rehearsal that confirms the capability works after a recovery boot.
- Or a release-gate run that exercises the capability against a production-shaped data set.

**Required artifacts:**

- All `E3` requirements.
- At least one of: receipt `E2E_PASSED`, receipt `RECOVERY_REHEARSAL_PASSED`, receipt `RELEASE_GATE_PASSED`.

**Verification:** S2.4 verification primitive chain composing the above receipts; explicit acceptance-criteria coverage check.

**Staleness:** an `E4` claim is stale if the relevant receipt is older than 30 days OR older than the most recent change to dependencies.

### 3.6 `E5` — live operational

The capability is in active production use; recent operational evidence shows it working.

**Required artifacts:**

- All `E4` requirements.
- A rolling window of `OPERATIONAL_HEALTHY` receipts emitted by the running system at no less than daily cadence.

**Verification:**

```text
AllOf [
  // E4 chain inherited
  evidence_exists(receipt_kind=OPERATIONAL_HEALTHY, capability_id=X, age < 24h),
  // Rolling window: at least 7 OPERATIONAL_HEALTHY receipts in the last 14 days
  count(receipts where kind=OPERATIONAL_HEALTHY and capability_id=X and age < 14d) >= 7
]
```

**Staleness:** an `E5` claim is stale if no `OPERATIONAL_HEALTHY` receipt has been emitted in the last 24 hours, or if the rolling-window minimum is not met.

## 4. Grade subsumption

```text
E5  ⊃  E4  ⊃  E3  ⊃  E2  ⊃  E1  ⊃  E0
```

A capability at `E5` MUST satisfy `E4`'s artifact requirements (which subsume `E3`'s, etc.). The grade of a capability is the highest grade for which all artifacts pass verification.

When checking whether a capability meets a required grade `R`:

1. Verify all artifact requirements for `R`.
2. Recursively verify all artifact requirements for `R-1`, `R-2`, ..., `E1`.
3. Reject if any subsumed requirement fails (the higher grade is invalid without the lower).

## 5. Grade computation

The current grade for a capability `X` is computed by:

```text
function compute_grade(X):
   if E5_conditions_met(X): return E5
   if E4_conditions_met(X): return E4
   if E3_conditions_met(X): return E3
   if E2_conditions_met(X): return E2
   if E1_conditions_met(X): return E1
   return E0
```

The check function for each grade is deterministic given the evidence log state. Computation is performed by the L0 governance service on demand and on evidence-log change events.

## 6. Grade and status mapping

The grade-to-status mapping is part of S6.1 §10 (status taxonomy). Reproduced here for convenience:

| Status     | Minimum grade                                                     |
| ---------- | ----------------------------------------------------------------- |
| `REAL`     | `E3` (or `E4`+ for recovery-critical / release-gate capabilities) |
| `PARTIAL`  | `E2`                                                              |
| `SHELL`    | `E1`                                                              |
| `CONTRACT` | `E1`                                                              |
| `DEFERRED` | `E0`                                                              |
| `BLOCKED`  | `E0`                                                              |
| `UNKNOWN`  | `E0`                                                              |
| `RETIRED`  | `E1`                                                              |

A status whose claimed level exceeds the capability's actual grade triggers the `STATUS_GRADE_CONSISTENT` property in S2.4 and emits `TAMPER_DETECTED` evidence (S3.1, FOREVER retention).

## 7. Recovery-critical and release-gate capabilities

Some capabilities require `E4` to claim `REAL`:

- L1 recovery path
- L4 vault broker (when refined)
- L4 emergency override (when refined)
- L4 policy bundle distribution
- S0.1 action envelope (touches every action; recovery sensitivity)
- S3.1 evidence log (constitutional integrity)
- L8 network policy (host-level enforcement)

The list of recovery-critical capabilities is maintained as a signed bundle (`reccritical_<hex>`) at L0. Adding to the list is a recovery-mode operation. Removing from the list is a recovery-mode operation. The list at any moment is canonical.

For capabilities on this list, gate G6 (per S6.1) requires `E4` instead of `E3`.

## 8. Evidence record `ARTIFACT_RECORDED`

Added to S3.1 RecordType vocabulary with `STANDARD_24M` retention:

```proto
message ArtifactRecordedPayload {
  string capability_id = 1;
  string artifact_kind = 2;            // closed enum (closed catalog)
  string aiosfs_pointer = 3;
  bytes content_hash = 4;              // hex_lower(BLAKE3(content))[:32]
  string producer = 5;                  // canonical_subject_id of producer
  google.protobuf.Timestamp recorded_at = 6;
}

enum ArtifactKind {
  ARTIFACT_KIND_UNSPECIFIED = 0;
  SPEC_DOCUMENT = 1;
  PROTO_IDL = 2;
  SOURCE_FILE = 3;
  TEST_FILE = 4;
  CONFIG_FILE = 5;
  BUILD_LOG = 6;
  TEST_LOG = 7;
  E2E_LOG = 8;
  RECOVERY_LOG = 9;
  RELEASE_GATE_LOG = 10;
  OPERATIONAL_TELEMETRY = 11;
  RETIREMENT_RECORD = 12;
  DEFERRAL_RECORD = 13;
}
```

## 9. Evidence record `BUILD_PASSED`, `TEST_PASSED`, `E2E_PASSED`, `RECOVERY_REHEARSAL_PASSED`, `RELEASE_GATE_PASSED`, `OPERATIONAL_HEALTHY`

Six new record types added to S3.1 vocabulary:

| Record type                 | Retention class | Carries                                                     |
| --------------------------- | --------------- | ----------------------------------------------------------- |
| `BUILD_PASSED`              | STANDARD_24M    | build-system id, exit code, capability ids built            |
| `TEST_PASSED`               | STANDARD_24M    | test-runner id, test-name list, exit code, capability_ids   |
| `E2E_PASSED`                | EXTENDED_60M    | scenario name, actor identity, action chain reference       |
| `RECOVERY_REHEARSAL_PASSED` | FOREVER         | recovery scenario id, operator, system snapshot             |
| `RELEASE_GATE_PASSED`       | FOREVER         | release id, gate definition reference, capability ids gated |
| `OPERATIONAL_HEALTHY`       | STANDARD_24M    | capability_id, health-probe definition, measurements        |

`RECOVERY_REHEARSAL_PASSED` and `RELEASE_GATE_PASSED` are FOREVER-retained because they are constitutional checkpoints — a recovery rehearsal failure is a load-bearing operational fact that must remain auditable.

## 10. Adversarial robustness

### 10.1 Forged evidence

All evidence receipts are signed per S3.1 §7 (per-segment Ed25519). A receipt with an invalid signature fails verification and is excluded from grade computation.

### 10.2 Replay attack

S3.1 receipt ids are unique per record (ULID). Replaying a receipt does not increase the grade — it only re-references the same artifact.

### 10.3 Stale evidence

Each grade has a staleness window (per §3). The grade computation function checks `age < window` for every contributing receipt. Stale receipts are excluded from the grade.

### 10.4 Cross-capability contamination

A `TEST_PASSED` receipt naming capability `A` does not contribute to capability `B`'s grade, even if `B` shares code with `A`. Each capability accumulates evidence under its own id; receipts must explicitly name the capability id.

### 10.5 Builder fraud

A producer claiming `BUILD_PASSED` without an actual build is caught by the `STATUS_GRADE_CONSISTENT` property in S2.4 — if subsequent verification (test runs, e2e probes) cannot succeed against the artifact, the inflated grade is detected and `TAMPER_DETECTED` evidence is emitted.

### 10.6 AI-self-grading

An AI subject cannot emit `BUILD_PASSED`, `TEST_PASSED`, etc. receipts that name itself as producer if the receipt is for code the AI authored. The L0 governance service rejects such receipts with `AgentSelfGradingBlocked` (rule name aligned with S2.3 §17 PascalCase + record-stem-form discipline; the FOREVER record itself is `AGENT_SELF_GRADING_BLOCKED`). Consequence: a CI system run on behalf of a human operator emits the receipts; AI subjects are not authorized producers for grade-promotion receipts. This complements S6.1 §7 (AI cannot mark its own status `REAL`).

## 11. Cross-spec dependencies

| Spec | Direction  | What this spec contributes                                                                                                                                                                                       |
| ---- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S3.1 | producer   | seven new record types (`ARTIFACT_RECORDED`, `BUILD_PASSED`, `TEST_PASSED`, `E2E_PASSED`, `RECOVERY_REHEARSAL_PASSED` FOREVER, `RELEASE_GATE_PASSED` FOREVER, `OPERATIONAL_HEALTHY`); closed `ArtifactKind` enum |
| S2.4 | producer   | property `STATUS_GRADE_CONSISTENT` (already in §17 from S6.1 touch-up; verifies grade-status alignment)                                                                                                          |
| S2.1 | producer   | `capability.evidence_grade` is a closed query field with `=`/`!=`/`<`/`<=`/`>`/`>=`/`in`                                                                                                                         |
| S6.1 | constraint | grade-to-status mapping consumed by gates G2..G6 and G10                                                                                                                                                         |

## 12. Golden fixtures

### Fixture 1 — Grade promotion through the chain

```text
T0: capability "S2.3" — evidence: 0 receipts → grade E0.
T1: spec file committed → ARTIFACT_RECORDED (kind=SPEC_DOCUMENT) → grade E1.
T2: proto-IDL compile → BUILD_PASSED → grade E2.
T3: unit tests added & pass → TEST_PASSED → grade E3.
T4: end-to-end policy decision flow exercised → E2E_PASSED → grade E4.
T5: production deployment + 7 OPERATIONAL_HEALTHY receipts in 14d → grade E5.
```

### Fixture 2 — Stale evidence drops grade

```text
Capability at E3 with TEST_PASSED receipt 5 days old.
Source file modified at T+0.
At T+1 second, grade recomputed: TEST_PASSED is now older than the source change.
   → grade drops to E2.
   → S6.1 G7 fires: capability transitions REAL → UNKNOWN until re-tested.
```

### Fixture 3 — Forged grade caught

```text
Receipt claims BUILD_PASSED for capability X, but signature does not verify.
   → receipt excluded from grade computation.
   → grade computed without it.
   → If receipt was the only BUILD_PASSED → grade falls to E1.
```

### Fixture 4 — AI cannot grade its own work

```text
Producer: family:family-assistant (is_ai=true).
Receipt: TEST_PASSED for capability "S5.1.identity_model".
Authorship check: family-assistant authored part of S5.1.
   → Receipt rejected at append with AgentSelfGradingBlocked.
   → No grade contribution.
```

### Fixture 5 — Recovery-critical requires E4 for REAL

```text
Capability "L1.recovery_path" on the recovery-critical bundle.
Grade currently E3 (TEST_PASSED but no RECOVERY_REHEARSAL_PASSED).
Proposed transition: PARTIAL → REAL via G6.
   → G6 checks recovery-critical bundle membership → requires E4.
   → Current grade E3 < E4 required → G6 fails with InsufficientEvidenceGrade.
   → Status remains PARTIAL.
```

### Fixture 6 — Cross-capability contamination prevented

```text
TEST_PASSED receipt names capability X.
Capability Y shares the same code module.
   → Y's grade does not increase.
   → Each capability requires its own TEST_PASSED naming itself.
```

## 13. Telemetry contract

All metrics MUST use bounded label cardinality. **capability_id, producer_canonical_id, receipt_id are NEVER labels.**

| Metric                                         | Type    | Labels (closed)                                                           |
| ---------------------------------------------- | ------- | ------------------------------------------------------------------------- |
| `evidence_grade_promotion_total`               | counter | `from_grade`, `to_grade`                                                  |
| `evidence_grade_demotion_total`                | counter | `from_grade`, `to_grade`, `reason_class` (stale/forged/dependency_failed) |
| `evidence_grade_distribution`                  | gauge   | `grade` (closed enum)                                                     |
| `evidence_artifact_recorded_total`             | counter | `artifact_kind` (closed enum)                                             |
| `evidence_signature_failure_total`             | counter | `receipt_kind` (closed enum)                                              |
| `evidence_producer_self_grade_rejection_total` | counter | `receipt_kind`                                                            |

## 14. Acceptance criteria

- [ ] `EvidenceGrade` is a closed enum with six values.
- [ ] Each grade has an artifact-list verification per §3.
- [ ] Grades subsume one another per §4; higher implies lower.
- [ ] Grade computation is deterministic given evidence log state.
- [ ] Grade-to-status mapping per §6 is enforced by S6.1 gates.
- [ ] Recovery-critical and release-gate capabilities require `E4` for `REAL` (§7).
- [ ] AI subjects cannot emit grade-promotion receipts for code they authored (§10.6).
- [ ] All six golden fixtures (§12) produce the specified outcomes.
- [ ] Telemetry conforms to §13; capability/producer/receipt ids never appear as labels.

## 15. Open deferrals

- **Recovery-critical bundle authoring tooling** — deferred to L9 admin operations sub-spec.
- **Distributed grade computation across machines** — deferred (Rev.2 is single-host).
- **Evidence sampling for grade computation** when receipt count is enormous — deferred; current contract evaluates all receipts naming the capability.
- **Incentive system for filling gaps** (e.g., flagging E2 capabilities to encourage E3) — deferred.

## See also

- [S6.1 — Status Taxonomy](01_status_taxonomy.md)
- [S6.4 — Constitutional Invariants](04_invariants.md)
- [S2.4 — Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [Rev.1 §7 — Governance and Evidence](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [L0 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
