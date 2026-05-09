# Status Taxonomy (Rev.2)

| Field          | Value                                                                                                    |
| -------------- | -------------------------------------------------------------------------------------------------------- |
| Status         | `CONTRACT` (initial; written 2026-05-09)                                                                 |
| Phase tag      | S6.1                                                                                                     |
| Layer          | L0 Governance, Evidence, Safety                                                                          |
| Schema package | `aios.governance.v1alpha1`                                                                               |
| Consumes       | nothing (L0 is the bottom of the dependency stack)                                                       |
| Produces       | the canonical status enum and transition rules used by every spec, evidence record, and operator surface |

## 1. Purpose

Every AIOS capability has a status that operators, evidence consumers, and other capabilities reason about. Without a fixed taxonomy, "done" means whatever the latest document or commit message says — and that drift defeats the constitutional core (`No proof = no completion`). This spec fixes the closed status vocabulary, the transitions between statuses, the evidence grade required to claim each status, and the gates that enforce those requirements.

## 2. Core invariants

- **I1 — Closed vocabulary.** Status is one of exactly eight values. Adding a value requires a versioned spec change (`v1alpha1` → `v1alpha2` etc.).
- **I2 — Evidence-bound promotion.** A status transition that increases capability (`SHELL` → `PARTIAL` → `REAL`) requires the receiving status's minimum evidence grade (per S6.2 evidence grades) to be present. Promotions without evidence are rejected at gate time.
- **I3 — No status equals proof.** `REAL` status without `E3` or higher evidence is rejected. `CONTRACT` without `E1` is rejected. Status claims are mechanical checks against evidence, not narrative.
- **I4 — Demotion is honest.** A capability that fails verification, loses its evidence, or has its dependencies retired is demoted automatically. Demotion is not a punishment — it is the natural state when proof goes away.
- **I5 — Status is per-spec, not per-layer.** A layer like L6 can hold simultaneously `REAL`/`PARTIAL`/`SHELL` sub-specs. The layer's headline status is computed from sub-specs (per §8), not authored independently.

## 3. The eight statuses

```proto
enum CapabilityStatus {
  CAPABILITY_STATUS_UNSPECIFIED = 0;
  REAL = 1;
  PARTIAL = 2;
  SHELL = 3;
  CONTRACT = 4;
  DEFERRED = 5;
  BLOCKED = 6;
  UNKNOWN = 7;
  RETIRED = 8;
}
```

### 3.1 `REAL`

The capability is implemented, exercised, and proven. The implementation matches the spec's acceptance criteria. Evidence at grade `E3` minimum is recorded; for capabilities affecting recovery or production, `E4` or `E5` is required.

**Use when:** a capability is in production use AND its acceptance criteria pass under current conditions.

**Required evidence grade:** `E3` minimum, `E4`+ for recovery/release-critical capabilities (cf. §10).

**Visible to users:** as a working feature.

### 3.2 `PARTIAL`

The capability exists and is exercised, but does not yet meet all acceptance criteria. Some flows succeed; others are stubbed, broken, or unverified.

**Use when:** at least one acceptance criterion fails, OR coverage of acceptance criteria is incomplete, OR known regressions exist.

**Required evidence grade:** `E2` (build passes) at minimum, plus enumerated list of which acceptance criteria currently fail.

**Visible to users:** as a known-incomplete feature with a list of unsupported scenarios.

### 3.3 `SHELL`

The capability has a placeholder presence — a folder, a stub file, an interface declaration — but no functioning implementation behind it.

**Use when:** scaffolding has been created (file path, function signature, page route) but the body is empty or returns a hard-coded placeholder.

**Required evidence grade:** `E1` (artifact exists).

**Visible to users:** internally as a planning marker; never surfaced as a feature.

### 3.4 `CONTRACT`

The capability has an approved specification but no implementation yet. The contract is implementer-grade: closed vocabularies, schemas, acceptance criteria, golden fixtures, performance budgets.

**Use when:** the spec is reviewed and approved but no code has been written.

**Required evidence grade:** `E1` (the spec file itself; possibly proto-IDL compile check).

**Visible to users:** internally as committed design; not surfaced as a feature.

### 3.5 `DEFERRED`

The capability is intentionally out of scope for the current revision. The decision is recorded; the work is not blocked, just postponed.

**Use when:** the work is recognized as needed but consciously moved to a later revision or release.

**Required evidence grade:** `E0` (no evidence required; the deferral itself is the artifact).

**Visible to users:** internally as a roadmap entry; not surfaced as a feature.

### 3.6 `BLOCKED`

The capability cannot proceed because of an external dependency, an upstream decision, or a hard constraint that has not been resolved.

**Use when:** progress is impossible until something outside the capability is unblocked. The blocker MUST be named explicitly.

**Required evidence grade:** `E0`, plus a named blocker reference (an evidence receipt id, an issue id, a spec id, or a vendor dependency).

**Visible to users:** internally as a flagged item; surfaced to operators if the blocker is in their control.

### 3.7 `UNKNOWN`

The capability's current status cannot be determined from available evidence. Either evidence is missing, evidence is inconclusive, or the capability has not been examined since changes elsewhere may have affected it.

**Use when:** a capability has not been re-verified after a system change that could plausibly affect it, AND no fresh evidence exists.

**Required evidence grade:** `E0`. The lack of evidence IS the reason for `UNKNOWN`.

**Visible to users:** internally as a flagged item that needs verification before any other status claim.

### 3.8 `RETIRED`

The capability has been intentionally removed or replaced. It is no longer part of the active system but is retained in the evidence trail.

**Use when:** the capability is decommissioned. A retirement record (when, why, replaced by what) is kept.

**Required evidence grade:** `E1` (the retirement record).

**Visible to users:** historically only; not surfaced as a feature.

## 4. Status comparison and ordering

Status is **not totally ordered**. There is a partial order on the "capability" axis:

```text
SHELL  <  PARTIAL  <  REAL          (capability axis; left = less capable)
```

`CONTRACT`, `DEFERRED`, `BLOCKED`, `UNKNOWN`, and `RETIRED` are **off the capability axis**:

- `CONTRACT` is design-complete but not yet building anything.
- `DEFERRED` is conscious non-action.
- `BLOCKED` is involuntary non-action.
- `UNKNOWN` is "I don't know yet".
- `RETIRED` is gone.

Comparisons that mix axes return `INCOMPARABLE`. The only "is this better than that" comparisons are within the capability axis.

## 5. Status transitions

```text
                       (E1)              (E2 + tests started)         (E3 + acceptance passing)
   ┌─────────┐  spec ┌──────────┐ build ┌─────────┐ verify ┌────────┐
   │  SHELL  │ ────▶│ CONTRACT │ ────▶│ PARTIAL │ ─────▶│  REAL  │
   └─────────┘       └──────────┘       └─────────┘        └────────┘
        │                  │                 │                  │
        │                  │                 ▼                  │
        │                  │             ┌──────┐              │
        │                  │             │ FAIL │ ◀────────────┘
        │                  │             └──────┘   (regression / evidence loss)
        │                  ▼
        │              ┌──────────┐
        ▼              │ DEFERRED │
   ┌─────────┐         └──────────┘
   │ UNKNOWN │              │
   └─────────┘              │
        ▲                   ▼
        │              ┌──────────┐
        │              │ BLOCKED  │
        │              └──────────┘
        │
   ┌──────────┐
   │ RETIRED  │ ◀─── any status (one-way; no return)
   └──────────┘
```

### 5.1 Allowed transitions

| From       | To         | Required evidence/cond.                                                      | Gate |
| ---------- | ---------- | ---------------------------------------------------------------------------- | ---- |
| `UNKNOWN`  | any        | re-examination produces evidence; new status follows that evidence           | G1   |
| `SHELL`    | `CONTRACT` | spec written and approved (E1)                                               | G2   |
| `SHELL`    | `DEFERRED` | explicit deferral decision recorded                                          | G3   |
| `CONTRACT` | `PARTIAL`  | implementation begun, build passes (E2), at least one acceptance test exists | G4   |
| `CONTRACT` | `DEFERRED` | scope decision retracted                                                     | G3   |
| `CONTRACT` | `BLOCKED`  | named external blocker arrives                                               | G5   |
| `PARTIAL`  | `REAL`     | all acceptance criteria pass (E3+), no open regressions                      | G6   |
| `PARTIAL`  | `BLOCKED`  | named external blocker arrives                                               | G5   |
| `PARTIAL`  | `UNKNOWN`  | evidence becomes stale (system changed; no re-verification yet)              | G7   |
| `REAL`     | `PARTIAL`  | regression detected; one or more acceptance criteria now fail                | G8   |
| `REAL`     | `UNKNOWN`  | system change makes prior evidence stale                                     | G7   |
| `REAL`     | `RETIRED`  | capability decommissioned                                                    | G9   |
| `BLOCKED`  | `CONTRACT` | blocker resolved; spec still good                                            | G10  |
| `BLOCKED`  | `PARTIAL`  | blocker resolved; implementation already partial                             | G10  |
| `BLOCKED`  | `DEFERRED` | scope decision retracted                                                     | G3   |
| `DEFERRED` | `CONTRACT` | revision opens; deferral lifted                                              | G2   |
| `DEFERRED` | `RETIRED`  | abandoned                                                                    | G9   |
| any        | `RETIRED`  | one-way decommission                                                         | G9   |

### 5.2 Forbidden transitions

The following transitions are **rejected** at gate time. Attempts to make them in evidence records produce `STATUS_TRANSITION_FORBIDDEN` errors.

- `SHELL → REAL` (cannot skip CONTRACT and PARTIAL).
- `SHELL → PARTIAL` (cannot skip CONTRACT).
- `CONTRACT → REAL` (cannot skip PARTIAL — implementation must be exercised before claiming reality).
- `RETIRED → anything` (one-way; revival requires a new spec id).
- Any non-`UNKNOWN` status → `UNKNOWN` without an explicit evidence-stale trigger.

### 5.3 Automatic transitions

Two transitions are automatic and do not require operator action:

- **`REAL` → `PARTIAL` on regression.** When an automated check (S2.4 verification primitive, scheduled property audit) fails, the affected capability is automatically demoted to `PARTIAL`. Evidence is emitted (`STATUS_AUTODEMOTED` record).
- **`REAL` → `UNKNOWN` on dependency change.** When a depended-on capability changes status (e.g., a dependency is retired or demoted), the dependent's prior evidence is invalidated and its status moves to `UNKNOWN` until re-verified.

Operators cannot disable automatic transitions; the discipline is constitutional.

## 6. Gates

A gate is a check function that returns `PASS` or `FAIL` for a proposed transition. All gates are deterministic given inputs.

```proto
enum GateId {
  GATE_ID_UNSPECIFIED = 0;
  G1_REEXAMINE = 1;
  G2_SPEC_APPROVED = 2;
  G3_DEFERRAL_RECORDED = 3;
  G4_IMPLEMENTATION_BEGUN = 4;
  G5_BLOCKER_RECORDED = 5;
  G6_ACCEPTANCE_PASSING = 6;
  G7_EVIDENCE_STALE = 7;
  G8_REGRESSION_DETECTED = 8;
  G9_DECOMMISSION_APPROVED = 9;
  G10_BLOCKER_RESOLVED = 10;
}
```

### 6.1 Gate inputs

Every gate takes:

- `from_status` and `to_status`
- `current_evidence_grade` and `required_evidence_grade`
- `acceptance_criteria_results` (list of pass/fail per criterion)
- `dependencies_status` (list of dependency capability statuses)
- `blocker_id` (optional)
- `actor` (canonical_subject_id of the operator proposing the transition)

### 6.2 Gate outputs

Either:

- `PASS` with the new status applied and a `STATUS_TRANSITION` evidence record emitted (STANDARD_24M retention).
- `FAIL` with a closed reason code (`InsufficientEvidenceGrade`, `AcceptanceCriteriaUnmet`, `BlockerMissing`, `DependencyDemoted`, `TransitionForbidden`, `ActorUnauthorized`).

### 6.3 Gate authorization

Gate G2 (spec approval), G3 (deferral), G6 (acceptance passing), and G9 (decommission) require an actor with the appropriate role:

- **G2 spec approval:** the spec author OR a designated reviewer for the layer; `kind = HUMAN_USER` only.
- **G3 deferral:** project lead role; `kind = HUMAN_USER` only.
- **G6 acceptance passing:** automated evidence is sufficient; manual marking is rejected (cf. §7).
- **G9 decommission:** project lead role; `kind = HUMAN_USER` only.

Gates G1, G4, G5, G7, G8, G10 are automatic — they fire when their preconditions are met, without operator action.

## 7. AI cannot mark its own status `REAL`

A constitutional invariant: an AI subject (`is_ai = true` per L4 identity) cannot pass gate G6 (acceptance passing) for a capability the AI itself produced. Gate G6 verifies the actor against the capability's authorship trail; if any AI subject contributed code AND the same AI subject is the actor proposing `REAL`, the gate returns `FAIL` with `ActorUnauthorized` and a sub-reason `AISelfStatusPromotionBlocked`.

This is the L0 mirror of S2.3 §17 (AI self-approval prevention) for the status taxonomy axis. AI may produce evidence; AI may build artifacts; AI cannot mark its own work as proven.

## 8. Layer and bundle headline status

A multi-spec layer (e.g., L4 with five sub-specs) does not carry an authored status. Its headline status is computed from sub-specs:

```text
layer.headline_status =
   RETIRED       if all sub-specs are RETIRED
   REAL          if all non-DEFERRED sub-specs are REAL
   PARTIAL       if any sub-spec is PARTIAL or any sub-spec is REAL while others are CONTRACT/SHELL
   CONTRACT      if all non-DEFERRED non-RETIRED sub-specs are CONTRACT
   SHELL         if at least one sub-spec is SHELL and none is PARTIAL or REAL
   BLOCKED       if any sub-spec is BLOCKED and the rest are non-PARTIAL/non-REAL
   UNKNOWN       otherwise
```

`DEFERRED` sub-specs are excluded from the rollup unless every sub-spec is `DEFERRED`, in which case the layer is `DEFERRED`.

The same rollup applies to bundles like "Phase 3 contracts" or "all of Rev.2".

## 9. Status carrying in evidence and queries

### 9.1 Evidence record `STATUS_TRANSITION`

Added to S3.1 RecordType vocabulary with `STANDARD_24M` retention:

```proto
message StatusTransitionPayload {
  string capability_id = 1;             // e.g., "S2.3", "L4.identity_model"
  CapabilityStatus from_status = 2;
  CapabilityStatus to_status = 3;
  GateId gate = 4;
  string evidence_grade_present = 5;    // E0..E5
  string evidence_grade_required = 6;   // E0..E5
  repeated string acceptance_criteria_failures = 7;
  string blocker_id = 8;                 // empty if not BLOCKED
  string actor_canonical_id = 9;
  string reason = 10;                    // free text from operator (or "automatic" for gate-fired)
  google.protobuf.Timestamp transitioned_at = 11;
}
```

### 9.2 Status as queryable field

S2.1 query language gains a closed query field `capability.status` with operators `=`, `!=`, `in`. This enables operator queries like "list all `BLOCKED` capabilities" or "show every capability whose status changed in the last 24h".

### 9.3 Evidence-grade-status invariant property

S2.4 verification grammar gains a property added to the closed `PropertyType` enum: `STATUS_GRADE_CONSISTENT`. The property checks that for every capability with a current `CapabilityStatus`, the evidence grade present meets the §3 minimum for that status. Failure → `TAMPER_DETECTED` evidence (capability claimed without proof).

## 10. Required evidence grade per status

| Status     | Minimum grade | Notes                                                              |
| ---------- | ------------- | ------------------------------------------------------------------ |
| `REAL`     | `E3`          | `E4`+ required for recovery-critical and release-gate capabilities |
| `PARTIAL`  | `E2`          | plus enumerated list of failing acceptance criteria                |
| `SHELL`    | `E1`          | placeholder file/route/signature exists                            |
| `CONTRACT` | `E1`          | spec file with proto IDL compile check is sufficient               |
| `DEFERRED` | `E0`          | the deferral decision IS the evidence                              |
| `BLOCKED`  | `E0`          | named blocker reference required                                   |
| `UNKNOWN`  | `E0`          | absence of evidence IS the reason                                  |
| `RETIRED`  | `E1`          | retirement record (when, why, replaced by what) required           |

The grade-to-status mapping is the formal contract. `E0..E5` definitions live in S6.2 evidence grades.

## 11. Adversarial robustness

### 11.1 Status injection from evidence

Status fields in evidence are written only by the gate evaluator. Direct writes to status by adapters or apps are rejected at the evidence log layer (the `STATUS_TRANSITION` payload is signed by the gate evaluator's key; mismatched signatures are rejected at append).

### 11.2 Evidence-grade lying

A claimed grade is verified by S2.4 verification primitives (`evidence_exists`, `policy_decision`, etc.). The `STATUS_GRADE_CONSISTENT` property in §9.3 catches any capability with a status that exceeds its actual evidence.

### 11.3 Race conditions

Two simultaneous `REAL` transitions for the same capability resolve via S1.3 optimistic concurrency. The losing actor receives `StatusTransitionInFlight` and re-fetches state.

### 11.4 Layered demotion

When a dependency's status drops, dependents become `UNKNOWN` (cf. §5.3). The propagation is bounded — direct dependents only — to prevent system-wide demotion cascades. Indirect dependents are re-evaluated lazily on next examination.

### 11.5 Bundle freezing

A signed status bundle (`statbundle_<hex>`) snapshots all capabilities' statuses at a point in time. Used for release gating: a bundle is signed when a release candidate is built; subsequent demotions do not invalidate the signed bundle's representation, but they do produce `STATUS_BUNDLE_DRIFT` evidence (FOREVER retention).

## 12. Cross-spec dependencies

| Spec | Direction  | What this spec contributes                                                                                          |
| ---- | ---------- | ------------------------------------------------------------------------------------------------------------------- |
| S2.1 | producer   | `capability.status` is a closed query field with `=`/`!=`/`in`                                                      |
| S2.4 | producer   | new property `STATUS_GRADE_CONSISTENT`                                                                              |
| S3.1 | producer   | new record types `STATUS_TRANSITION` STANDARD_24M, `STATUS_AUTODEMOTED` STANDARD_24M, `STATUS_BUNDLE_DRIFT` FOREVER |
| L4   | constraint | gate G6 requires actor signature; AI subjects rejected per §7                                                       |

## 13. Golden fixtures

### Fixture 1 — `SHELL` to `CONTRACT` to `PARTIAL` to `REAL`

```text
T0: capability "S7.1.fictional" at SHELL, evidence E1 (placeholder file).
T1: spec written and approved by HUMAN_USER reviewer → G2 fires → CONTRACT.
T2: implementation begins, build passes (E2), one acceptance test exists → G4 fires → PARTIAL.
T3: all acceptance criteria pass under E3 evidence → G6 fires → REAL.
   STATUS_TRANSITION evidence emitted at each step.
```

### Fixture 2 — Automatic demotion on regression

```text
Capability "S5.1.identity_model" at REAL, E3.
S2.4 scheduled property POLICY_AI_SELF_APPROVAL_BLOCKED audit fails for an action.
   → G8 fires automatically → REAL → PARTIAL.
   STATUS_AUTODEMOTED evidence emitted with the failing property reference.
```

### Fixture 3 — AI cannot mark its own work `REAL`

```text
Actor: family:family-assistant (AI_AGENT, is_ai=true)
Capability: "S5.1.identity_model" at PARTIAL
Proposed transition: PARTIAL → REAL (G6)

Authorship trail of S5.1: includes commits authored by family:family-assistant.
   → Gate G6 fails with ActorUnauthorized + sub-reason AISelfStatusPromotionBlocked.
   → Status remains PARTIAL.
   STATUS_TRANSITION evidence emitted with reason "AI self-promotion blocked".
```

### Fixture 4 — Forbidden transition

```text
Capability at SHELL.
Proposed transition: SHELL → REAL (skipping CONTRACT and PARTIAL).
   → All gates reject → STATUS_TRANSITION_FORBIDDEN.
   → Status remains SHELL.
```

### Fixture 5 — Layer rollup

```text
L4 sub-specs:
  01_policy_kernel.md     REAL
  02_vault_broker.md      SHELL
  03_identity_model.md    REAL
  04_approval_mechanics.md SHELL
  05_emergency_override.md SHELL

Rollup: PARTIAL  (some specs REAL, some SHELL; not all REAL → not REAL; some PARTIAL/REAL → not pure SHELL)
```

### Fixture 6 — `STATUS_GRADE_CONSISTENT` property catches lying

```text
Capability claimed REAL.
Evidence trail: only E1 receipts; no E3 acceptance evidence.
S2.4 property STATUS_GRADE_CONSISTENT fires.
   → TAMPER_DETECTED evidence (FOREVER retention).
   → Operator-visible alert.
   → Capability auto-demoted to UNKNOWN until reconciled.
```

## 14. Telemetry contract

All metrics MUST use bounded label cardinality. **capability_id, actor_canonical_id, blocker_id are NEVER labels.**

| Metric                                       | Type    | Labels (closed)                                                              |
| -------------------------------------------- | ------- | ---------------------------------------------------------------------------- |
| `governance_status_transition_total`         | counter | `from_status`, `to_status`, `gate`, `result` (pass/fail)                     |
| `governance_status_autodemotion_total`       | counter | `from_status`, `reason_class` (regression/dependency_demoted/evidence_stale) |
| `governance_status_grade_inconsistent_total` | counter | `claimed_status`, `actual_max_grade`                                         |
| `governance_active_capabilities`             | gauge   | `status` (closed enum)                                                       |
| `governance_blocked_capabilities`            | gauge   | none                                                                         |
| `governance_unknown_capabilities`            | gauge   | none                                                                         |
| `governance_bundle_drift_total`              | counter | none                                                                         |

## 15. Acceptance criteria

- [ ] `CapabilityStatus` is a closed enum with eight values; adding a value requires a versioned spec change.
- [ ] Status transitions follow the §5 table; forbidden transitions are rejected at gate time.
- [ ] Each status has a minimum required evidence grade per §10.
- [ ] AI subjects cannot pass gate G6 for capabilities they authored (§7).
- [ ] Automatic demotion fires on regression (G8) and dependency status change (G7).
- [ ] Layer headline status is computed from sub-specs (§8), not authored.
- [ ] `STATUS_GRADE_CONSISTENT` property catches any capability with status exceeding its evidence grade.
- [ ] All six golden fixtures (§13) produce the specified outcomes.
- [ ] Telemetry conforms to §14; capability/actor/blocker ids never appear as labels.

## 16. Open deferrals

- **Status bundle signing protocol details** — `statbundle_<hex>` snapshot mechanics deferred to L9 release/admin sub-specs.
- **Indirect dependent re-evaluation strategy** — current contract: lazy on next examination. Eager propagation deferred.
- **Status SLOs** (e.g., "no capability stays UNKNOWN longer than 14 days") — deferred to L9 admin operations sub-spec.
- **External dependency tracking** (third-party libraries marked as their own capabilities) — deferred.

## See also

- [S6.2 — Evidence Grades](02_evidence_grades.md)
- [S6.4 — Constitutional Invariants](04_invariants.md)
- [S2.4 — Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [Rev.1 §6 — Status taxonomy origin](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [L0 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
