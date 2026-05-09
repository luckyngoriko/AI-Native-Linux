# AIOS-SGR State Transitions and Graph Evaluation (Rev.2)

| Field          | Value                                                                                                                                                                      |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists; structural contract complete)                                                                              |
| Phase tag      | S15.2                                                                                                                                                                      |
| Layer          | L3 AIOS-SGR / Capability Runtime                                                                                                                                           |
| Schema package | `aios.sgr.v1alpha1`                                                                                                                                                        |
| Consumes       | S15.1 Unit Manifest (sibling sub-spec; referenced abstractly via `UnitManifest`), S10.1 Capability Runtime gRPC, S0.1 Action Envelope + Lifecycle, S2.4 Verification       |
|                | Engine, S3.1 Evidence Log RecordType + retention vocabulary, S3.2 Sandbox Composition, S14.1 Failure Handling (recovery-loop pattern donor), L0 invariants `INV-007`,      |
|                | `INV-014`                                                                                                                                                                  |
| Produces       | closed `GraphEvaluationResult` (5 values), closed `TransitionKind` (10 values), closed `DependencySolveResult` (4 values), closed `ABPromotionState` (5 values), the       |
|                | deterministic graph evaluation algorithm with content-addressed `transition_plan_id`, the A/B promotion FSM with N=3 success / N=2 failure thresholds, the resource budget |
|                | composition rule (most-restrictive-wins), the per-subject transition rate limit, twelve evidence record types queued for S3.1, performance budgets, three worked examples  |

## §1 Purpose

This sub-spec defines **how AIOS-SGR turns desired state into action**. It specifies the deterministic graph-evaluation algorithm that compares the active runtime graph against the operator-declared desired graph, the closed enumeration of transition kinds the runtime can plan, the dependency solver that orders those transitions safely, the A/B promotion finite-state machine that governs service updates, the resource-budget composition rule that prevents transition starvation, and the adversarial-robustness guarantees that bound graph evaluation in the presence of cycles, contradictory requests, and rate-limit abuse.

The unit manifest (S15.1) describes **what** a service is. The Capability Runtime gRPC (S10.1) describes **how** an individual typed action is dispatched, validated, executed, verified, and rolled back. This sub-spec sits between them: it describes **when** transitions are planned, **in what order** they are applied, and **on what evidence** the runtime declares that the live system has converged on the desired state.

This file defines:

1. The closed `GraphEvaluationResult` enum (five values) and its membership rules.
2. The closed `TransitionKind` enum (ten values) and the per-kind preconditions.
3. The closed `DependencySolveResult` enum (four values) and the cycle-detection contract.
4. The closed `ABPromotionState` enum (five values) and the A/B promotion FSM.
5. The deterministic graph-evaluation algorithm and the content-addressed `transition_plan_id`.
6. The resource budget composition rule (most-restrictive-wins across desired graph, manifest, sandbox profile, and policy floor).
7. The per-subject transition rate-limit contract and the `TransitionConflict` rejection class.
8. Performance budgets for graph evaluation and per-`TransitionKind` application.
9. Twelve evidence record types queued for S3.1 with retention class.
10. Adversarial robustness rules (cycles, conflicts, spam, manifest forgery, contradictory bundles).
11. Three worked examples covering web-service A/B upgrade, dependency wait + resolution, and conflict detection.

This file does **not** define:

- The unit manifest schema itself — that is S15.1 (`01_unit_manifest.md`).
- The gRPC dispatch surface or the action-lifecycle FSM — that is S10.1 (`03_capability_runtime_grpc.md`).
- Per-adapter target schemas — that is S15.4 (`04_adapter_model.md`).
- The action envelope shape, request hash, dry-run semantics — that is S0.1.
- The verification primitive vocabulary, EBNF, or property checks — that is S2.4.
- The sandbox profile shape or composition algorithm — that is S3.2.
- The evidence log hash chain, segment lifecycle, or query API — that is S3.1.
- The recovery boot path or generic-fallback kernel — that is L1.

## §2 Scope

### §2.1 In scope

- The closed `GraphEvaluationResult`, `TransitionKind`, `DependencySolveResult`, `ABPromotionState` enums.
- The deterministic graph evaluation algorithm with content-addressed `transition_plan_id`.
- The dependency solver, including topological ordering, cycle detection, and waiting semantics.
- The A/B promotion FSM with N=3 health-check success threshold (promote) and N=2 health-check failure threshold (rollback).
- Resource budget composition (most-restrictive-wins across four sources).
- Per-subject transition rate-limiting and contradictory-transition rejection.
- The full record vocabulary added to S3.1 by this sub-spec (twelve record types, queued).
- Performance budgets for evaluation and per-kind application.
- Adversarial robustness fixtures for cycles, conflicts, spam, forgery.
- Three worked examples end-to-end.

### §2.2 Out of scope

- The unit manifest schema (S15.1). When this file references "manifest", it means "an S15.1 `UnitManifest` record" without redefining it.
- The action dispatch surface, lifecycle, queue classes (S10.1).
- Per-adapter shape (S15.4).
- Wire shape of action envelope (S0.1).
- Verification grammar (S2.4) — the evaluator delegates to S2.4 for health-probe primitives.
- Sandbox profile composition internals (S3.2) — this sub-spec only consumes the composed `SandboxProfile` and the floor signature.
- Multi-host SGR federation — Rev.2 mandates a single authoritative SGR per host.
- Migration of state across hosts — deferred (see §13).

## §3 Vocabulary

This section declares the closed enums on which the rest of the sub-spec is built. Every enum is contract-grade. Adding a value is a versioned spec change. Bundle load fails on unknown values. Wire compatibility follows S0.1 §8.

### §3.1 `GraphEvaluationResult`

Closed enum, five values. This is the result of one complete graph evaluation pass — the comparison of live runtime graph against the desired graph that produces a `TransitionPlan`.

```proto
enum GraphEvaluationResult {
  GRAPH_EVALUATION_RESULT_UNSPECIFIED = 0;
  CONVERGED = 1;
  IN_PROGRESS = 2;
  BLOCKED_DEPENDENCY = 3;
  BLOCKED_RESOURCE = 4;
  FAILED = 5;
}
```

| Value                | Semantics                                                                                                                                                                                                                                                                                                                                                                                               |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `CONVERGED`          | The live graph hash equals the desired graph hash modulo the closed equivalence relation defined in §5.2. The plan is empty (`transitions = []`). This is the steady-state outcome.                                                                                                                                                                                                                     |
| `IN_PROGRESS`        | The live graph and desired graph differ; the plan is non-empty; every transition in the plan has all dependencies satisfied or a documented `WAITING` ancestor; the plan can begin applying transitions.                                                                                                                                                                                                |
| `BLOCKED_DEPENDENCY` | The live and desired graphs differ; one or more required transitions cannot be ordered because a prerequisite unit is itself in a non-`STABLE` state and cannot make progress. The evaluator emits `GRAPH_BLOCKED_DEPENDENCY` once per evaluation and does not apply any transitions until the dependency clears or the operator changes the desired graph.                                             |
| `BLOCKED_RESOURCE`   | The live and desired graphs differ; one or more required transitions cannot be applied because a resource budget (CPU, memory, GPU, sandbox-floor seat, network capability) would be exceeded. The composition rule (§6) is the source of truth. The evaluator emits `GRAPH_BLOCKED_RESOURCE` and waits for capacity to free or for the operator to modify the desired graph.                           |
| `FAILED`             | The graph evaluation itself failed (cycle detected, manifest schema violation, signature failure, evaluation budget exceeded). The evaluator emits a corresponding diagnostic record and refuses to produce a plan. `FAILED` is **never** a transient state for the same `(graph_state_hash, target_state_hash)` pair: re-evaluation with the same inputs is rejected from the result cache (see §5.3). |

The full closed list is exactly five values. Adding a sixth requires a versioned spec change.

### §3.2 `TransitionKind`

Closed enum, ten values. Every transition the SGR plans is exactly one of these kinds. The kind drives the per-kind preconditions, the verification template, the rollback strategy, and the performance budget.

```proto
enum TransitionKind {
  TRANSITION_KIND_UNSPECIFIED = 0;
  START = 1;
  STOP = 2;
  RESTART = 3;
  UPGRADE_AB_PROMOTE = 4;
  UPGRADE_AB_ROLLBACK = 5;
  SCALE_UP = 6;
  SCALE_DOWN = 7;
  RECONFIGURE = 8;
  DEPENDENCY_REORDER = 9;
  NO_OP = 10;
}
```

| Value                 | Semantics                                                                                                                                                                                                                                                                                                                                            |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `START`               | The unit is absent in the live graph or in a `STOPPED` state and the desired graph requires it to be in `RUNNING` state. Verification: post-start health probe (S2.4 primitive). Rollback: `STOP`.                                                                                                                                                   |
| `STOP`                | The unit is `RUNNING` in the live graph and is not present in the desired graph (or is desired-stopped). Verification: post-stop absence check. Rollback: re-`START` from the prior unit version.                                                                                                                                                    |
| `RESTART`             | The unit's image hash, configuration hash, or sandbox profile hash has changed and the operator has annotated the change as restart-only (no A/B). Single-process replacement; brief unavailability is permitted by manifest. Verification: post-start health probe. Rollback: re-restart with prior version.                                        |
| `UPGRADE_AB_PROMOTE`  | The unit has a new desired image/config and the manifest annotates it as A/B-eligible. Variant B is started alongside the live A; on N=3 successful health checks the runtime promotes B to A; on N=2 failures it discards B without affecting A. See §7.                                                                                            |
| `UPGRADE_AB_ROLLBACK` | The promotion FSM enters its rollback path: variant B is stopped and removed, variant A continues serving. Emitted when N=2 consecutive failed health checks land on B before promotion. The `UPGRADE_AB_ROLLBACK` is not a "compensating" transition for a successful `UPGRADE_AB_PROMOTE`; it is the failure-path exit of the same promotion FSM.  |
| `SCALE_UP`            | The desired replica count exceeds the live replica count for a unit declared horizontally scalable in its manifest. Verification: post-start health probe on each new replica. Rollback: `SCALE_DOWN` to prior count.                                                                                                                                |
| `SCALE_DOWN`          | The desired replica count is below the live replica count. Replicas drained per manifest's drain protocol. Verification: post-stop absence check on the removed replicas. Rollback: `SCALE_UP` to prior count.                                                                                                                                       |
| `RECONFIGURE`         | A configuration-only change (no image hash change, no sandbox profile change) for a unit that exposes runtime reconfiguration via its manifest. Distinct from `RESTART` because no process replacement occurs. Verification: configuration probe (S2.4). Rollback: re-apply prior configuration.                                                     |
| `DEPENDENCY_REORDER`  | The dependency graph between live units changed (new edge, removed edge, edge re-ordering) without unit content changing. The runtime adjusts startup order metadata for the next reboot/restart cycle but performs no live process action. Verification: structural check on the persisted graph metadata. Rollback: re-write prior graph metadata. |
| `NO_OP`               | The desired graph differs from live in a way the evaluator considers cosmetic (e.g., manifest comment-field changes that do not impact runtime behavior). Recorded for evidence; no action dispatched. Verification: graph-hash comparison succeeds against the canonicalized form. No rollback (no effect).                                         |

The full closed list is exactly ten values plus the sentinel. Adding an eleventh kind requires a versioned spec change. The evaluator rejects manifests that imply an out-of-vocabulary transition (`TransitionKindUnknown` evidence; see §9.5).

### §3.3 `DependencySolveResult`

Closed enum, four values. This is the result of solving the dependency edges between transitions in a single evaluation pass.

```proto
enum DependencySolveResult {
  DEPENDENCY_SOLVE_RESULT_UNSPECIFIED = 0;
  SATISFIED = 1;
  WAITING = 2;
  IMPOSSIBLE = 3;
  CYCLE = 4;
}
```

| Value        | Semantics                                                                                                                                                                                                                                                                                                                                                                                                   |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SATISFIED`  | All dependency edges between planned transitions have been ordered into a directed acyclic plan; no transition will dispatch before its prerequisites are at the required runtime state.                                                                                                                                                                                                                    |
| `WAITING`    | At least one transition has a prerequisite that is itself in a non-terminal state (e.g., A/B promotion in `CANARY`). The plan can run the un-blocked tail; the waiting transitions are deferred to the next evaluation round.                                                                                                                                                                               |
| `IMPOSSIBLE` | A required prerequisite cannot exist (e.g., a transition depends on a unit that is `RETIRED` and the manifest does not provide a successor). The plan is rejected; `GRAPH_EVALUATION_RESULT = BLOCKED_DEPENDENCY` is emitted with `dependency_result = IMPOSSIBLE` in the diagnostic.                                                                                                                       |
| `CYCLE`      | The dependency graph contains a cycle. The evaluator emits `DEPENDENCY_CYCLE_DETECTED` (FOREVER retention; see §9.7) with the cycle nodes enumerated and rejects the plan with `GRAPH_EVALUATION_RESULT = FAILED`. Cycles are constitutional faults: the runtime fails closed and does not attempt heuristic edge-removal. The operator must change the desired graph or unit manifests to break the cycle. |

### §3.4 `ABPromotionState`

Closed enum, five values. This is the FSM state for an in-flight A/B promotion. The runtime persists this state per (`unit_id`, `target_image_hash`) pair in `/aios/system/sgr/promotions/<unit_id>/<plan_id>`.

```proto
enum ABPromotionState {
  AB_PROMOTION_STATE_UNSPECIFIED = 0;
  B = 1;
  CANARY = 2;
  A_PROMOTED = 3;
  STABLE = 4;
  ROLLBACK = 5;
}
```

| State        | Semantics                                                                                                                                                                                                                                                                                          |
| ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `B`          | Variant B has been started but is not yet receiving production traffic. The unit's traffic-routing rule still points entirely at variant A. Health probes are scheduled per the manifest cadence. Initial state of every promotion.                                                                |
| `CANARY`     | Variant B has passed at least one health probe and is receiving a manifest-declared fraction of production traffic (default 5%, manifest-overridable up to 50%). The runtime is accumulating success/failure counts toward the N=3 / N=2 thresholds (§7).                                          |
| `A_PROMOTED` | The N=3 successful health-check threshold has been reached. The runtime has switched 100% of production traffic to variant B and demoted A to standby. A is retained for one full health cadence to allow fast rollback. The transition kind dispatched at this state is `UPGRADE_AB_PROMOTE`.     |
| `STABLE`     | The post-promotion observation window has elapsed without failure. Variant A is reaped. The promotion FSM record for this `(unit_id, target_image_hash)` is sealed (`promotion_sealed = true`) and read-only thereafter. This is the terminal-success state.                                       |
| `ROLLBACK`   | The N=2 failed health-check threshold was reached on variant B (in `B` or `CANARY` state) before promotion. The runtime stops B, restores 100% traffic to A, and emits `UPGRADE_AB_ROLLBACK` plus `AB_ROLLBACK_PERFORMED` evidence. This is the terminal-failure state for this promotion attempt. |

The legal transitions are exactly:

```text
B          → CANARY    (1st successful health check)
B          → ROLLBACK  (2nd failed health check)
CANARY     → A_PROMOTED (3rd successful health check)
CANARY     → ROLLBACK   (2nd failed health check)
A_PROMOTED → STABLE     (post-promotion observation window elapsed; no rollback request)
A_PROMOTED → ROLLBACK   (operator-initiated rollback within observation window OR additional failure)
```

There is no transition from `ROLLBACK` or `STABLE` — both are terminal. A new promotion attempt requires a new plan with a new `transition_plan_id`. Re-using the same `(unit_id, target_image_hash)` after a `ROLLBACK` requires either a manifest change (different `target_image_hash`) or operator-acknowledged retry recorded in evidence (`AB_ROLLBACK_RETRY_ACKNOWLEDGED`).

This pattern mirrors the recovery-loop detector pattern donor in S14.1 §6.4: a small, integer threshold; counted within a bounded window; deterministic transition; one permanent record per terminal state. The thresholds (N=3 success, N=2 failure) are hard-coded in this spec and not bundle-overridable. The cadence between probes is manifest-declared but bounded `[5s, 60s]`.

## §4 Graph state representation

This section is brief because the graph schema lives in S15.1 (`UnitManifest`) and S4.1 (`/aios/system/sgr/...` namespace). It is restated here only to fix the hash domain.

The **runtime graph** is the live observed state: the set of units actually running on the host, their replica counts, image hashes, configuration hashes, sandbox profile hashes, and inter-unit dependency edges as observed at evaluation time `T`. It is denoted `G_live(T)`.

The **desired graph** is the operator-declared target state: the set of units the operator wants running, their replica counts, image hashes, configuration hashes, sandbox profile hashes, and inter-unit dependency edges as declared by the active `/aios/system/sgr/desired/` content at evaluation time. It is denoted `G_desired(T)`.

Both graphs share a closed canonicalization scheme:

1. **Field ordering**: alphabetical by field name within each unit; alphabetical by `unit_id` across units.
2. **Edge canonicalization**: dependency edges are emitted as `(src_unit_id, dst_unit_id, edge_kind)` triples; `edge_kind` is the closed S15.1 dependency-edge enum (deferred reference); ordering is alphabetical.
3. **Empty-field elision**: optional fields with default values are omitted before hashing so semantically identical graphs hash identically.
4. **Hash function**: BLAKE3 over the canonicalized JSON-canonical-serialization (JCS) bytes; lowercase hex.

The hash of `G_live(T)` is denoted `graph_state_hash`. The hash of `G_desired(T)` is denoted `target_state_hash`. Both are 64-character lowercase hex strings.

## §5 Graph evaluation algorithm

### §5.1 Determinism contract

Graph evaluation is deterministic: for the same `(graph_state_hash, target_state_hash)` pair, the same plan must be produced. This is the foundation of the content-addressed `transition_plan_id` (§5.3) and of the result cache that prevents `FAILED` evaluations from being silently retried.

Determinism implies:

1. The evaluator does not consult external clocks except to record evaluation start/end timestamps in evidence (these timestamps do **not** influence the plan content).
2. The evaluator does not consult random sources.
3. The evaluator does not consult the live evidence log; it is a pure function of `G_live`, `G_desired`, the active sandbox-floor signature `sigfloor_<hex>` (S3.2), and the active policy bundle hash (referenced for fail-closed checks, not for plan content).
4. The order of transitions in the plan is the unique topological order produced by Kahn's algorithm with tie-breaking on alphabetical `unit_id` then alphabetical `transition_kind`.
5. Two evaluations of the same `(graph_state_hash, target_state_hash)` produce byte-identical `TransitionPlan` records.

### §5.2 Equivalence relation for `CONVERGED`

The evaluator declares `CONVERGED` when `graph_state_hash == target_state_hash`. The canonicalization scheme (§4) is the equivalence relation: any two runtime states that canonicalize to the same hash are considered equivalent.

A `NO_OP`-bearing plan is **not** the same as `CONVERGED`. If the desired graph contains a comment-field change that canonicalization elides, the runtime graph and desired graph hash identically and the result is `CONVERGED` with an empty plan. If canonicalization does **not** elide the difference (e.g., a runtime-tracked label that influences observability but not behavior), the evaluator emits `IN_PROGRESS` with a `NO_OP` transition.

### §5.3 `transition_plan_id` and the result cache

The plan id is content-addressed:

```text
transition_plan_id = "tplan_" || hex_lower(BLAKE3(jcs(canonicalized_plan)))[:48]
```

Where `canonicalized_plan` includes:

- `graph_state_hash` (input)
- `target_state_hash` (input)
- `floor_signature` (`sigfloor_<hex>` from S3.2 active floor)
- `policy_bundle_hash` (active S2.3 bundle hash; included because policy can shift hard-deny vocabulary that would change `BLOCKED_*` outcomes; deterministic per (graph, target, floor, bundle) tuple)
- `evaluation_result` (`GraphEvaluationResult` enum value)
- `transitions[]` (ordered list of `TransitionRecord`)
- `dependency_result` (`DependencySolveResult` enum value)

The runtime maintains a **result cache** keyed by `(graph_state_hash, target_state_hash, floor_signature, policy_bundle_hash)`. On evaluation:

1. Look up the cache. If a cached `transition_plan_id` exists with `evaluation_result = FAILED`, return it directly. Re-evaluation with the same inputs is rejected.
2. If a cached plan exists with `evaluation_result ∈ {CONVERGED, IN_PROGRESS, BLOCKED_DEPENDENCY, BLOCKED_RESOURCE}`, return it directly **only** if the runtime is idle on the relevant subjects (no in-flight transitions touching the same units). Otherwise, recompute (a transition has changed the live graph since the cache entry was taken).
3. Otherwise, evaluate from scratch and write the result to the cache.

The cache is bounded (default 1024 entries; LRU eviction). Entries expire when any of `(graph_state_hash, target_state_hash, floor_signature, policy_bundle_hash)` changes. The cache is **not** persisted across runtime restarts; restarts produce fresh evaluations.

### §5.4 Algorithm in pseudocode

```text
Evaluate(G_live, G_desired, floor_sig, bundle_hash) -> EvaluationResult:
  1. h_live    = canonical_hash(G_live)
  2. h_desired = canonical_hash(G_desired)
  3. cache_key = (h_live, h_desired, floor_sig, bundle_hash)
  4. if cache.contains(cache_key) and cache_valid(cache_key):
       return cache.get(cache_key)

  5. if h_live == h_desired:
       plan = TransitionPlan(transitions=[], result=CONVERGED, dep_result=SATISFIED)
       emit GRAPH_CONVERGED evidence
       cache.put(cache_key, plan)
       return plan

  6. diff = compute_unit_diff(G_live, G_desired)
       # diff entries have closed kind: ADDED / REMOVED / CHANGED / REORDERED / COSMETIC

  7. transitions = []
     for entry in diff:
       kind = classify_transition(entry, G_live, G_desired)
         # may return TransitionKindUnknown -> emit TRANSITION_KIND_UNKNOWN evidence and FAIL
       transitions.append(make_transition(entry, kind))

  8. dep_result = solve_dependencies(transitions, G_live, G_desired)
       # returns SATISFIED | WAITING | IMPOSSIBLE | CYCLE

  9. if dep_result == CYCLE:
       emit DEPENDENCY_CYCLE_DETECTED evidence (FOREVER)
       plan = TransitionPlan(result=FAILED, dep_result=CYCLE, transitions=[],
                             diagnostic=enumerate_cycle_nodes())
       cache.put(cache_key, plan)
       return plan

 10. if dep_result == IMPOSSIBLE:
       plan = TransitionPlan(result=BLOCKED_DEPENDENCY, dep_result=IMPOSSIBLE,
                             transitions=[], diagnostic=name_missing_prereq())
       emit GRAPH_BLOCKED_DEPENDENCY evidence
       cache.put(cache_key, plan)
       return plan

 11. resource_result = check_resource_budget(transitions, G_live, manifests, floor_sig)
       # most-restrictive-wins across (desired_graph, manifest, sandbox_floor, policy_floor)

 12. if not resource_result.feasible:
       emit RESOURCE_BUDGET_DENIED evidence (EXTENDED_60M)
       emit GRAPH_BLOCKED_RESOURCE evidence (STANDARD_24M)
       plan = TransitionPlan(result=BLOCKED_RESOURCE, dep_result=dep_result,
                             transitions=[], diagnostic=resource_result.reason)
       cache.put(cache_key, plan)
       return plan

 13. transitions = topological_sort(transitions, dep_result)
       # Kahn; alphabetical (unit_id, transition_kind) tie-break

 14. plan = TransitionPlan(result=IN_PROGRESS, dep_result=dep_result,
                           transitions=transitions)
     emit GRAPH_EVALUATED evidence (STANDARD_24M)
     cache.put(cache_key, plan)
     return plan
```

### §5.5 Evaluation budget

A single evaluation must complete within the §8.1 budget. If the budget is exceeded (graph too large, pathological input), the evaluator emits `GRAPH_EVALUATION_BUDGET_EXCEEDED` evidence and returns `FAILED`. The threshold is configured per host but bounded `[100ms, 1000ms]`. Default 200ms. The budget check is a defensive measure; well-formed graphs evaluate in well under the §8.1 p95.

## §6 Resource budget composition

### §6.1 Sources

The composition rule fuses resource budgets from four sources, in this fixed order:

1. **Desired-graph aggregate** — sum across all units in `G_desired` of declared resource demand: CPU shares, memory bytes, GPU compute class quota (S8.2), sandbox-floor seats (per-class quotas defined in S3.2), network capability slots.
2. **Per-unit manifest declarations** — each unit's manifest (S15.1) declares its resource demand. The aggregate above is the sum of these; this source is the per-unit detail used for diagnostics.
3. **Sandbox profile floor (S3.2)** — the runtime safety floor signed as `sigfloor_<hex>` declares per-class hard maxima the runtime may not loosen. This includes per-class CPU/memory ceilings, per-class GPU compute restrictions (per `INV-024`), and per-class sandbox-floor seats.
4. **Policy bundle floor (S2.3)** — the active bundle may declare per-subject or per-group resource ceilings. These are evaluated as additional constraints; never as relaxations of the sandbox-floor.

### §6.2 Composition rule (most-restrictive-wins)

For each scalar resource dimension `R`:

```text
limit(R) = min(
  desired_graph.demand(R),
  manifest_aggregate.demand(R),
  sandbox_floor.max(R),
  policy_floor.max(R)
)
```

The **demand** must be ≤ all three floors **and** internally consistent (`desired_graph.demand` must equal `manifest_aggregate.demand` modulo replica multiplication; mismatch is a manifest-versus-desired inconsistency emitted as `RESOURCE_BUDGET_INCONSISTENT` evidence). For boolean / set-membership resources (e.g., `gpu.compute_heavy` capability), the rule is:

```text
allowed(R) = desired_graph.allowed(R)
            ∧ manifest_aggregate.allowed(R)
            ∧ sandbox_floor.allowed(R)
            ∧ policy_floor.allowed(R)
```

If any source forbids `R`, the resource is denied. This mirrors S3.2 §5 most-restrictive-wins with one additional source (the desired-graph aggregate) participating.

### §6.3 Composition outcomes

| Outcome              | Trigger                                                                                                  | Result                                                                                 |
| -------------------- | -------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `feasible`           | All scalar demands fit within all four sources; all boolean/set demands are allowed by all four sources. | Evaluation proceeds to §5 step 13 (sort + return `IN_PROGRESS`).                       |
| `over_budget_scalar` | `min(...)` returns a value strictly less than `desired_graph.demand` for at least one dimension.         | `BLOCKED_RESOURCE` with diagnostic `dimension`, `requested`, `available`.              |
| `forbidden_set`      | At least one boolean/set resource is forbidden by at least one source.                                   | `BLOCKED_RESOURCE` with diagnostic `resource_id`, `forbidden_by` (closed source list). |
| `inconsistent`       | `desired_graph.demand(R) ≠ sum(unit.demand(R) * replicas)` for some `R`.                                 | `FAILED` with diagnostic `RESOURCE_BUDGET_INCONSISTENT`.                               |

`BLOCKED_RESOURCE` is recoverable: the evaluator re-checks on the next evaluation and proceeds when capacity frees. `FAILED` (inconsistent) is a structural fault; the operator must repair the desired graph.

### §6.4 No starvation guarantee

The composition rule guarantees no transition can be queued indefinitely while live graph resources are sufficient. The evaluator does not pre-allocate; resources are checked against current `G_live` plus pending transitions. A transition that fits today and is blocked tomorrow only because another transition started in between is **not** starved — the next evaluation will re-check. The runtime does not implement priority inversion mitigations beyond the per-subject rate limit (§7.5).

## §7 A/B promotion FSM

### §7.1 Eligibility

A unit is A/B-eligible if its manifest declares `ab_eligible = true` and its `target_image_hash` differs from the live `image_hash`. Units with `ab_eligible = false` follow `RESTART` semantics (single-process replacement, brief unavailability).

### §7.2 Promotion sequence

```text
1.  Plan classification: TransitionKind = UPGRADE_AB_PROMOTE.
    Plan emits TRANSITION_QUEUED evidence (STANDARD_24M).

2.  Variant B start:
    - Allocate sandbox per S3.2 with the same per-class floor as A.
    - Apply manifest's startup sequence.
    - Persist ABPromotionState = B in /aios/system/sgr/promotions/<unit_id>/<plan_id>.
    - Schedule health probes per manifest cadence (bounded [5s, 60s]).
    - Emit TRANSITION_STARTED evidence.

3.  Health-check accumulation (state = B):
    - On each scheduled probe, run S2.4 health primitive.
    - On success: increment success_count.
    - On failure: increment failure_count.
    - When success_count == 1: transition state -> CANARY, route N% traffic to B
        (manifest-declared, default 5%, max 50%).
    - When failure_count == 2: transition state -> ROLLBACK (jump to step 6).

4.  Health-check accumulation (state = CANARY):
    - Continue scheduled probes against B with traffic share.
    - On success: increment success_count.
    - On failure: increment failure_count.
    - When success_count == 3: transition state -> A_PROMOTED.
    - When failure_count == 2: transition state -> ROLLBACK (jump to step 6).

5.  A_PROMOTED:
    - Switch 100% traffic to B.
    - Demote A to standby (alive but no traffic).
    - Hold for one full cadence (the post-promotion observation window).
    - Emit AB_CANARY_PROMOTED evidence (STANDARD_24M).
    - On observation-window elapse without rollback: transition -> STABLE.
    - Reap A. Seal promotion record (read-only).
    - Emit TRANSITION_SUCCEEDED evidence.

6.  ROLLBACK (terminal failure):
    - Stop variant B.
    - Restore 100% traffic to A.
    - Persist ABPromotionState = ROLLBACK.
    - Emit AB_ROLLBACK_PERFORMED evidence (FOREVER).
    - Emit TRANSITION_FAILED evidence (EXTENDED_60M) with reason = AB_HEALTHCHECK_THRESHOLD.
    - Plan dispatches a paired UPGRADE_AB_ROLLBACK transition kind for accountability.
```

### §7.3 Threshold rationale

The N=3 / N=2 thresholds are intentionally small integers and intentionally asymmetric:

- **N=3 successful health checks before promote** — three independent observations of healthy behavior across at least 3 × cadence (15s minimum). Three is the smallest count that distinguishes a stable signal from coincidence (one and two probes can both succeed by chance during a brief warm-up window).
- **N=2 failed health checks before rollback** — two consecutive failures across 2 × cadence (10s minimum). Two is the smallest count that distinguishes a real fault from a single transient probe error. Asymmetric thresholds (N=2 < N=3) mean failure is detected sooner than success is confirmed, reflecting that AIOS errs on the side of rollback.

The N values are **not** bundle-overridable. The cadence is manifest-overridable within `[5s, 60s]`. This is the same pattern as S14.1 §6.4 recovery-loop detection: small bounded thresholds, deterministic transition, FOREVER evidence on the terminal failure path.

### §7.4 Observation window

After `A_PROMOTED`, the runtime holds for one full cadence before reaping A. During this window:

- An operator-initiated rollback request transitions state to `ROLLBACK` and reactivates A.
- An additional health failure on B transitions state to `ROLLBACK` (rare; B has already passed three probes).
- Rollback during observation window emits `AB_OBSERVATION_WINDOW_ROLLBACK` evidence (FOREVER) — distinct from the pre-promote `AB_ROLLBACK_PERFORMED` because it represents post-promotion fault.

After observation window elapses without rollback: `STABLE`, A reaped, promotion record sealed.

### §7.5 Rate limiting

To prevent transition spamming, the runtime enforces a **per-subject transition rate limit**:

| Bucket             | Limit                                                                                              |
| ------------------ | -------------------------------------------------------------------------------------------------- |
| Per-subject window | 30 transitions per 60-second sliding window (across all `TransitionKind` values for that subject). |
| Per-unit window    | 5 transitions per 60-second sliding window (any kind, any subject).                                |
| Per-class window   | 100 `UPGRADE_AB_PROMOTE` transitions per 24-hour window globally.                                  |

Exceeding any bucket emits `TRANSITION_RATE_LIMITED` evidence (STANDARD_24M) and rejects the transition with `TransitionRateLimited`. The rejected action is **not** retried automatically; the operator must wait for the window to slide. The buckets are hard-coded in this spec and not bundle-overridable; they are part of the L0-aligned safety floor for SGR.

### §7.6 Concurrent contradictory transitions

A **contradictory transition** is two pending or in-flight transitions on the same `unit_id` whose ordered application would yield non-deterministic outcomes. Examples:

- `START` and `STOP` on the same unit submitted by different subjects.
- `UPGRADE_AB_PROMOTE` and `UPGRADE_AB_ROLLBACK` overlapping for the same unit.
- `RECONFIGURE` to two distinct configuration hashes with overlapping execution windows.

The runtime detects contradictory transitions at evaluation time. The conflict is rejected with `TransitionConflict` and emits `TRANSITION_CONFLICT` evidence (FOREVER). The first-submitted transition continues; the contradictory submission is rejected. The detection rule is:

```text
contradictory(t1, t2) ⟺
   t1.unit_id == t2.unit_id
   ∧ ¬(t1.kind == NO_OP ∨ t2.kind == NO_OP)
   ∧ ¬(t1 fully terminated before t2 dispatched)
   ∧ ¬compatible_kinds(t1.kind, t2.kind)
```

`compatible_kinds` is a closed lookup table: only `(SCALE_UP, SCALE_UP)` and `(SCALE_DOWN, SCALE_DOWN)` are compatible (multiple replicas can scale serially). All other pairs touching the same `unit_id` are contradictory.

### §7.7 FOREVER retention rationale

`AB_ROLLBACK_PERFORMED`, `AB_OBSERVATION_WINDOW_ROLLBACK`, `DEPENDENCY_CYCLE_DETECTED`, and `TRANSITION_CONFLICT` are FOREVER-retained because they are **constitutional evidence**: they record a runtime decision to refuse change-of-state in the face of a fault or conflict. Operators must be able to audit such events years after the fact.

## §8 Performance contract

### §8.1 Graph evaluation

| Metric           | Budget       | Source                                   |
| ---------------- | ------------ | ---------------------------------------- |
| `Evaluate` p50   | < 8ms        | typical 50-unit graph, local manifests   |
| `Evaluate` p95   | < 50ms       | mandated; exceeding triggers fail-closed |
| `Evaluate` p99   | < 200ms      | budget exceeded -> FAILED + diagnostic   |
| `Evaluate` worst | enforced cap | `GRAPH_EVALUATION_BUDGET_EXCEEDED`       |

The p95 < 50ms budget is the headline figure cited in the executive summary: the runtime can evaluate the desired-vs-live state of a hundred-unit-class host within one display-frame budget. This is a deliberate choice — the SGR is a control loop, not a batch process.

### §8.2 Per-`TransitionKind` application

Per-kind budgets are end-to-end from `TRANSITION_QUEUED` to terminal `TRANSITION_SUCCEEDED` / `TRANSITION_FAILED`:

| `TransitionKind`      | p95 budget | Notes                                                                 |
| --------------------- | ---------- | --------------------------------------------------------------------- |
| `START`               | < 3s       | dominated by sandbox composition + process spawn + first health probe |
| `STOP`                | < 2s       | drain protocol per manifest                                           |
| `RESTART`             | < 5s       | start + stop in series                                                |
| `UPGRADE_AB_PROMOTE`  | < 30s      | dominated by N=3 health checks at default cadence (5s × 3 + buffer)   |
| `UPGRADE_AB_ROLLBACK` | < 5s       | stop B + restore traffic                                              |
| `SCALE_UP`            | < 3s × N   | per replica (linear)                                                  |
| `SCALE_DOWN`          | < 2s × N   | per replica (linear, drain-bound)                                     |
| `RECONFIGURE`         | < 1s       | configuration probe; no process replacement                           |
| `DEPENDENCY_REORDER`  | < 100ms    | metadata write; no live action                                        |
| `NO_OP`               | < 50ms     | evidence emit only                                                    |

Budget exceedance emits `TRANSITION_BUDGET_EXCEEDED` evidence (EXTENDED_60M; queued for S3.1 as a Wave 6 candidate; not enumerated in §9 below to keep the twelve-record set minimal). The evaluator does not abort; it lets the dispatch finish and reports the deviation as observability data.

## §9 Evidence record types (queued for S3.1)

This sub-spec queues twelve closed `RecordType` values for S3.1's vocabulary. Each entry names retention class and a one-line semantic. Payloads carry the standard back-reference (`action_id`, `transition_plan_id`, `unit_id` as applicable) plus the kind-specific fields enumerated below.

| #   | RecordType                  | Retention      | Trigger                                                                                                                            |
| --- | --------------------------- | -------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| 1   | `GRAPH_EVALUATED`           | `STANDARD_24M` | A graph evaluation completed with result `IN_PROGRESS`. Carries `transition_plan_id`, transition count, dependency_result.         |
| 2   | `TRANSITION_QUEUED`         | `STANDARD_24M` | A transition was added to the dispatch queue. Carries `transition_id`, `transition_kind`, `unit_id`.                               |
| 3   | `TRANSITION_STARTED`        | `STANDARD_24M` | A transition was dispatched to the adapter. Carries `transition_id`, dispatch start timestamp.                                     |
| 4   | `TRANSITION_SUCCEEDED`      | `STANDARD_24M` | A transition reached its terminal-success state. Carries `transition_id`, verification result hash.                                |
| 5   | `TRANSITION_FAILED`         | `EXTENDED_60M` | A transition reached terminal-failure (verification failed, adapter error, budget exceeded). Carries `transition_id`, reason.      |
| 6   | `AB_CANARY_PROMOTED`        | `STANDARD_24M` | An A/B promotion FSM transitioned `CANARY -> A_PROMOTED`. Carries `unit_id`, success_count, target_image_hash.                     |
| 7   | `AB_ROLLBACK_PERFORMED`     | `FOREVER`      | An A/B promotion FSM transitioned to `ROLLBACK`. Carries `unit_id`, failure_count, prior_image_hash, target_image_hash.            |
| 8   | `DEPENDENCY_CYCLE_DETECTED` | `FOREVER`      | The dependency solver detected a cycle. Carries `cycle_nodes[]`, `cycle_edges[]`, evaluation_input_hash.                           |
| 9   | `TRANSITION_CONFLICT`       | `FOREVER`      | Two contradictory transitions detected on the same unit. Carries `winning_transition_id`, `rejected_transition_id`, conflict_kind. |
| 10  | `RESOURCE_BUDGET_DENIED`    | `EXTENDED_60M` | A transition was rejected by §6 composition. Carries `resource_dimension`, requested, available, source_blocking.                  |
| 11  | `GRAPH_BLOCKED_RESOURCE`    | `STANDARD_24M` | An evaluation returned `BLOCKED_RESOURCE`. Carries `transition_plan_id`, blocking dimension, blocking source.                      |
| 12  | `GRAPH_CONVERGED`           | `STANDARD_24M` | An evaluation returned `CONVERGED`. Carries `graph_state_hash`, `target_state_hash` (equal), evaluation duration.                  |

The retention assignments follow S3.1's vocabulary discipline:

- Routine lifecycle events (`GRAPH_EVALUATED`, `TRANSITION_QUEUED`, `TRANSITION_STARTED`, `TRANSITION_SUCCEEDED`, `AB_CANARY_PROMOTED`, `GRAPH_BLOCKED_RESOURCE`, `GRAPH_CONVERGED`) → `STANDARD_24M`.
- Failure-relevant events (`TRANSITION_FAILED`, `RESOURCE_BUDGET_DENIED`) → `EXTENDED_60M` (longer than routine because they support root-cause analysis windows).
- Constitutional refusals (`AB_ROLLBACK_PERFORMED`, `DEPENDENCY_CYCLE_DETECTED`, `TRANSITION_CONFLICT`) → `FOREVER` (per `INV-014` and the FOREVER pattern for refused-state-change events).

These twelve records are added to the S3.1 closed `RecordType` enum at S3.1 consolidation; their payload schemas are defined in the S3.1 `RecordPayload` discriminated oneof.

## §10 Adversarial robustness

### §10.1 Cycle detection

A cycle in the dependency graph (any cycle, any length) causes the solver to return `CYCLE`. The evaluator:

1. Refuses to apply any transitions in this evaluation pass.
2. Emits `DEPENDENCY_CYCLE_DETECTED` (FOREVER) with the enumerated cycle nodes and edges.
3. Enters the cache as a `FAILED` evaluation; same `(graph_state_hash, target_state_hash)` will not be re-evaluated.
4. Surfaces the cycle to the operator via `aios admin sgr show-cycles` (deferred to S15.5 admin tooling).

The operator must change at least one of the cycle's edges (typically by editing a unit manifest's `requires =` declaration) and submit a new desired graph. There is **no heuristic edge-removal**: the runtime does not "guess" which edge to drop.

### §10.2 Transition spam

A subject submitting transitions faster than the §7.5 buckets allow has each surplus transition rejected with `TransitionRateLimited`. The rejection emits `TRANSITION_RATE_LIMITED` evidence and increments `sgr_transition_rate_limited_total{subject_kind, bucket}`.

Spam mitigation interacts with S2.3's policy decisions: a rate-limited subject is not policy-denied (the policy decision was `ALLOW` in the normal case), but the SGR's local rate limit fires after policy. This preserves the principle that rate limits are runtime resource discipline, not policy decisions, and keeps the evidence chain unambiguous: `POLICY_DECISION = ALLOW` plus `TRANSITION_RATE_LIMITED`.

### §10.3 Concurrent contradictory transitions

Per §7.6, two contradictory transitions on the same `unit_id` are rejected with `TransitionConflict`. The first-submitted transition wins; the second is rejected and emits `TRANSITION_CONFLICT` (FOREVER). The first-submitted-wins rule uses the L3 dispatch queue arrival timestamp (recorded in `TRANSITION_QUEUED` evidence), not envelope wall-clock — clock skew between subjects cannot be exploited to "win" a conflict.

### §10.4 Manifest forgery

A unit manifest with an incorrect or missing signature is rejected at S15.1 manifest load time and never enters the desired graph. The evaluator does not see forged manifests. If a forged manifest somehow reaches evaluation (e.g., due to evidence-log tamper recovery in progress), the evaluator detects the missing signature and emits `UNIT_MANIFEST_FORGERY_DETECTED` (queued for S15.1). This is out of scope for this sub-spec but called out for cross-reference.

### §10.5 Bundle/floor swapping

If the active sandbox-floor signature `sigfloor_<hex>` or the active policy bundle hash changes between evaluation passes, the result-cache key changes and the evaluator recomputes from scratch. This guarantees that a freshly loaded floor cannot be silently bypassed by a cached pre-floor evaluation result.

### §10.6 `BLOCKED_*` recoverability

`BLOCKED_DEPENDENCY` and `BLOCKED_RESOURCE` are recoverable: re-evaluation on the next tick (default cadence 1s, manifest-overridable in `[500ms, 5s]`) will retry. `FAILED` is not recoverable for the same `(graph_state_hash, target_state_hash)` pair: the operator must change at least one input. This asymmetry mirrors S14.1 §6.4 recovery-loop detection: routine setbacks recover, structural faults require human intervention.

### §10.7 Layer dependency invariant (`INV-007`)

This sub-spec consumes only L0 (invariants), L1 (substrate references), L2 (AIOS-FS namespace for promotion records), and same-layer L3 sub-specs (S15.1, S10.1). It does **not** consume any L4..L10 spec for correctness. Conformance with `INV-007` is verified by L0's architectural-audit step that scans the "Consumes" header. Any future addition to this sub-spec's consumes list must lie in `{L0, L1, L2, L3}`.

### §10.8 Evidence completeness invariant (`INV-014`)

This sub-spec emits twelve evidence record types (§9); every terminal state of every transition produces at least one record; every refused-state-change produces a FOREVER record. Together, this ensures that a status claim of `REAL` for the SGR control loop is backed by an unbroken evidence chain from `GRAPH_EVALUATED` through `TRANSITION_QUEUED → TRANSITION_STARTED → TRANSITION_SUCCEEDED` (or its failure-path counterparts). Verification is via S2.4 property `STATUS_GRADE_CONSISTENT` (existing) augmented with a scheduled audit that walks `transition_plan_id` chains.

## §11 Worked examples

### §11.1 Example 1 — Web service A/B upgrade

**Setup.** Unit `web.frontend` (web app, manifest declares `ab_eligible = true`, `health_probe.cadence_s = 5`). Live state: image hash `img_a`, configuration hash `cfg_v1`. Operator pushes desired graph: image hash `img_b`, configuration hash `cfg_v1`. No other units change.

**Trace.**

```text
T+0.000  Operator pushes desired graph. New target_state_hash differs from graph_state_hash.
T+0.012  Evaluate() called. Cache miss. Diff = [{web.frontend: CHANGED (image)}].
T+0.013  classify_transition -> UPGRADE_AB_PROMOTE.
T+0.014  solve_dependencies -> SATISFIED (no other units affected).
T+0.015  check_resource_budget -> feasible (B fits within floor for the unit's class).
T+0.016  TransitionPlan{result=IN_PROGRESS, transitions=[t1: UPGRADE_AB_PROMOTE]}.
T+0.017  GRAPH_EVALUATED evidence emitted.
T+0.020  TRANSITION_QUEUED evidence emitted (t1).
T+0.030  Adapter dispatches; sandbox composed; B started.
T+0.080  TRANSITION_STARTED evidence emitted; ABPromotionState=B; success_count=0.
T+0.100  Probe scheduled at T+5.000.
T+5.005  Health probe 1: PASS. success_count=1. State -> CANARY. Route 5% traffic to B.
T+10.010 Health probe 2: PASS. success_count=2.
T+15.015 Health probe 3: PASS. success_count=3. State -> A_PROMOTED. Switch 100% to B.
T+15.016 AB_CANARY_PROMOTED evidence emitted.
T+20.020 Observation window elapsed; no rollback request; state -> STABLE.
T+20.030 A reaped; promotion record sealed.
T+20.040 TRANSITION_SUCCEEDED evidence emitted (t1).
T+20.050 Next Evaluate() call: graph_state_hash equals target_state_hash; CONVERGED.
T+20.060 GRAPH_CONVERGED evidence emitted.
```

**End state.** Live image hash = `img_b`. Promotion FSM record sealed. Total elapsed time ≈ 20 seconds (within the 30-second §8.2 budget for `UPGRADE_AB_PROMOTE`).

### §11.2 Example 2 — Dependency wait + resolution

**Setup.** Unit `db.postgres` requires `network.policy_loaded = true` (manifest dependency edge). Live state: `db.postgres` not running; `network.policy_loaded` is in `B` state (mid-promotion). Operator submits desired graph requiring `db.postgres` running.

**Trace.**

```text
T+0.000  Evaluate() called.
         G_live: {db.postgres: STOPPED, network.policy_loaded: B}
         G_desired: {db.postgres: RUNNING, network.policy_loaded: STABLE}

T+0.020  Diff = [{db.postgres: CHANGED (state)}, {network.policy_loaded: CHANGED (state)}].
T+0.021  classify_transition:
           - db.postgres -> START
           - network.policy_loaded -> already in promotion FSM (no new transition; observable)
T+0.022  solve_dependencies:
           - db.postgres requires network.policy_loaded == STABLE
           - network.policy_loaded.state == B (not terminal)
           - WAITING.
T+0.023  TransitionPlan{result=IN_PROGRESS, transitions=[]}, dep_result=WAITING.
T+0.024  GRAPH_EVALUATED evidence emitted with dep_result=WAITING.

T+0.500  Next evaluation tick. network.policy_loaded.state == CANARY (still not terminal).
         dep_result=WAITING. No transitions dispatched. GRAPH_EVALUATED emitted.

T+5.005  network.policy_loaded promotion completes; state == STABLE.
T+5.500  Next evaluation tick.
         G_live: {db.postgres: STOPPED, network.policy_loaded: STABLE}
         G_desired matches except db.postgres.
T+5.520  Diff = [{db.postgres: CHANGED (state)}].
T+5.521  classify_transition: START.
T+5.522  solve_dependencies: SATISFIED (network.policy_loaded.state == STABLE).
T+5.523  check_resource_budget: feasible.
T+5.524  TransitionPlan{result=IN_PROGRESS, transitions=[t1: START db.postgres]}.
T+5.525  TRANSITION_QUEUED, TRANSITION_STARTED, ... TRANSITION_SUCCEEDED.
T+8.500  CONVERGED. GRAPH_CONVERGED emitted.
```

**End state.** Both units in desired state. The waiting period emitted multiple `GRAPH_EVALUATED` records with `dep_result=WAITING`; downstream observability tooling (S15.5) can render these as a single "waiting on `network.policy_loaded`" episode.

### §11.3 Example 3 — Conflict detection

**Setup.** Two operator subjects (operator-001 and operator-002) submit concurrent envelopes targeting the same unit `cache.redis`:

- operator-001 submits `START cache.redis` at T+0.000.
- operator-002 submits `STOP cache.redis` at T+0.005.

**Trace.**

```text
T+0.000  operator-001's envelope arrives. policy_decision=ALLOW.
         Plan classified: TransitionKind=START. transition_id=t1.
         TRANSITION_QUEUED evidence emitted (queue arrival timestamp t_q1=T+0.001).

T+0.002  Adapter dispatched for t1. State machine for cache.redis enters START dispatch.
         TRANSITION_STARTED emitted.

T+0.005  operator-002's envelope arrives. policy_decision=ALLOW.
         Plan classified: TransitionKind=STOP. transition_id=t2.
         Evaluator checks contradiction:
           contradictory(t1, t2)?
             t1.unit_id == t2.unit_id == cache.redis ✓
             neither is NO_OP ✓
             t1 not yet terminated ✓
             compatible_kinds(START, STOP) == false ✓
           -> contradictory.

T+0.006  TransitionConflict raised. t2 rejected.
         TRANSITION_CONFLICT evidence emitted (FOREVER):
           winning_transition_id = t1 (queue arrival earlier)
           rejected_transition_id = t2
           conflict_kind = INCOMPATIBLE_KINDS
           subjects = [operator-001, operator-002]

T+1.000  t1 completes: TRANSITION_SUCCEEDED.
T+1.020  Next Evaluate(): G_live includes cache.redis: RUNNING; G_desired (per
         persisted desired graph) still says RUNNING. CONVERGED. GRAPH_CONVERGED.

T+5.000  operator-002 inspects evidence; sees TRANSITION_CONFLICT; submits a fresh
         STOP envelope. New transition t3 is now non-contradictory (t1 terminated long
         ago); proceeds normally. (But this requires changing the desired graph too;
         left to operator workflow tooling.)
```

**End state.** `cache.redis` running. The conflict event is permanently in the evidence log; future audits can reconstruct who attempted what and why operator-002's STOP was refused.

## §12 Cross-spec dependencies

| Spec  | Direction  | What this spec contributes                                                                                                                                                                                                                                                                        |
| ----- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S15.1 | consumer   | references `UnitManifest` schema, `ab_eligible`, `health_probe`, dependency-edge enum (when refined). Touch-up: S15.1 must declare `ab_eligible: bool`, `health_probe.cadence_s ∈ [5,60]`, `requires[]: list<DependencyEdge>`, `replicas: u32`, `ecosystem_runtime` (per existing S12.1 binding). |
| S10.1 | consumer   | references the closed `ActionLifecycleState` and the gRPC dispatch surface; this sub-spec emits transitions that S10.1 dispatches. No new RPC added.                                                                                                                                              |
| S0.1  | consumer   | references the action envelope shape; no new envelope fields added. Transition records carry the originating `action_id`.                                                                                                                                                                         |
| S2.4  | consumer   | references the verification primitive vocabulary for health probes. Touch-up queued: `transition_succeeded`, `transition_failed` primitive names already in S2.4 closed primitive enum.                                                                                                           |
| S3.1  | producer   | twelve new `RecordType` values (§9) queued for S3.1 vocabulary consolidation. Retention classes assigned per S3.1 discipline.                                                                                                                                                                     |
| S3.2  | consumer   | references the composed `SandboxProfile` and the floor signature `sigfloor_<hex>`; this sub-spec consumes both as inputs to §6.2 most-restrictive-wins. Adds the desired-graph aggregate as a fourth participant in the most-restrictive-wins composition.                                        |
| S14.1 | pattern    | the N=3/N=2 threshold-with-FOREVER-evidence pattern is a structural mirror of S14.1 §6.4 recovery-loop detection. No data dependency.                                                                                                                                                             |
| L0    | constraint | binds `INV-007` (layer dependency: this sub-spec consumes only L0..L3) and `INV-014` (no proof, no completion: every transition emits an evidence record).                                                                                                                                        |

## §13 Open deferrals

- **Multi-host SGR federation** — the desired graph is a single-host artifact in Rev.2. Federated desired graphs across a cluster of AIOS hosts are deferred. The plan id and result cache are local to one host.
- **Predictive scheduling** — pre-empting `BLOCKED_RESOURCE` by reserving capacity for queued plans is deferred. Rev.2 only checks at evaluation time.
- **Per-unit plan diffing** — a future tooling spec (S15.5) will render plan diffs ("show me what changed in the next plan vs the previous applied plan"). The plan id content-addressing makes this trivial; the surface is deferred.
- **Operator-rejected plan record** — when an operator inspects a plan and chooses not to apply it (manual review path), a `PLAN_REJECTED_BY_OPERATOR` evidence record may be queued in a future revision. Out of scope for the twelve-record vocabulary here.
- **Per-`TransitionKind` budget exceedance record** — `TRANSITION_BUDGET_EXCEEDED` is mentioned in §8.2 but not enumerated in §9; queued for Wave 6 of S3.1 vocabulary.
- **A/B promotion thresholds tunable per unit class** — currently N=3/N=2 are global. A future revision may allow per-class thresholds (e.g., critical-path units use N=5 success, N=2 failure). Out of scope for Rev.2.
- **Concurrent A/B promotions across multiple units** — currently independent per `unit_id`. Cross-unit coordination (atomic multi-unit upgrade) is deferred to a future S15.6.

## §13.1 Evidence payload schemas (queued for S3.1)

Each of the twelve record types in §9 carries a typed payload added to the S3.1 `RecordPayload` discriminated oneof at consolidation. The schemas below are normative for the S15.2 contributions; field types use the same primitives as the rest of Rev.2 (`string` = canonical id form, `bytes` = BLAKE3 hex on-the-wire, `Timestamp` = google.protobuf.Timestamp).

```proto
message GraphEvaluatedPayload {
  string transition_plan_id = 1;
  string graph_state_hash = 2;
  string target_state_hash = 3;
  string floor_signature = 4;
  string policy_bundle_hash = 5;
  GraphEvaluationResult result = 6;
  DependencySolveResult dependency_result = 7;
  uint32 transitions_planned = 8;
  uint64 evaluation_duration_us = 9;
}

message TransitionQueuedPayload {
  string transition_id = 1;
  string transition_plan_id = 2;
  TransitionKind kind = 3;
  string unit_id = 4;
  string action_id = 5;
  string subject_canonical_id = 6;
  Timestamp queued_at = 7;
}

message TransitionStartedPayload {
  string transition_id = 1;
  string adapter_id = 2;
  Timestamp dispatched_at = 3;
}

message TransitionSucceededPayload {
  string transition_id = 1;
  string verification_result_hash = 2;
  uint64 duration_ms = 3;
}

message TransitionFailedPayload {
  string transition_id = 1;
  TransitionFailureReason reason = 2;   // closed enum, declared in this sub-spec, mirrors S10.1 ExecutionFailureReason subset
  string diagnostic = 3;
  uint64 duration_ms = 4;
}

message AbCanaryPromotedPayload {
  string unit_id = 1;
  string transition_plan_id = 2;
  uint32 success_count = 3;             // always 3 at promotion time
  string prior_image_hash = 4;
  string target_image_hash = 5;
}

message AbRollbackPerformedPayload {
  string unit_id = 1;
  string transition_plan_id = 2;
  uint32 failure_count = 3;             // always 2 at rollback time
  ABPromotionState rollback_from = 4;   // B or CANARY (or A_PROMOTED for observation-window rollback)
  string prior_image_hash = 5;
  string target_image_hash = 6;
}

message DependencyCycleDetectedPayload {
  repeated string cycle_nodes = 1;
  repeated string cycle_edges = 2;      // formatted as "src->dst:edge_kind"
  string evaluation_input_hash = 3;     // hash of (graph_state_hash, target_state_hash, floor_signature, policy_bundle_hash)
}

message TransitionConflictPayload {
  string winning_transition_id = 1;
  string rejected_transition_id = 2;
  string unit_id = 3;
  ConflictKind conflict_kind = 4;       // closed enum: INCOMPATIBLE_KINDS, OVERLAPPING_RECONFIGURE, OVERLAPPING_AB
  Timestamp winning_queued_at = 5;
  Timestamp rejected_queued_at = 6;
  string winning_subject = 7;
  string rejected_subject = 8;
}

message ResourceBudgetDeniedPayload {
  string transition_id = 1;
  ResourceDimension dimension = 2;       // closed enum: CPU, MEMORY, GPU_COMPUTE, FLOOR_SEAT, NETWORK_CAPABILITY
  uint64 requested = 3;
  uint64 available = 4;
  ResourceSource source_blocking = 5;    // closed enum: DESIRED_GRAPH, MANIFEST, SANDBOX_FLOOR, POLICY_FLOOR
}

message GraphBlockedResourcePayload {
  string transition_plan_id = 1;
  ResourceDimension dimension = 2;
  ResourceSource source_blocking = 3;
}

message GraphConvergedPayload {
  string graph_state_hash = 1;          // == target_state_hash
  uint64 evaluation_duration_us = 2;
}
```

The payload-companion enums (`TransitionFailureReason`, `ConflictKind`, `ResourceDimension`, `ResourceSource`) are closed enums declared by this sub-spec and added to `aios.sgr.v1alpha1`. Their values are listed below.

```proto
enum TransitionFailureReason {
  TRANSITION_FAILURE_REASON_UNSPECIFIED = 0;
  ADAPTER_ERROR = 1;
  HEALTH_PROBE_TIMEOUT = 2;
  AB_HEALTHCHECK_THRESHOLD = 3;
  VERIFICATION_FAILED = 4;
  BUDGET_EXCEEDED = 5;
  ROLLED_BACK_BY_OPERATOR = 6;
  PRECONDITION_LOST = 7;                // a prerequisite changed state during dispatch
}

enum ConflictKind {
  CONFLICT_KIND_UNSPECIFIED = 0;
  INCOMPATIBLE_KINDS = 1;
  OVERLAPPING_RECONFIGURE = 2;
  OVERLAPPING_AB = 3;
}

enum ResourceDimension {
  RESOURCE_DIMENSION_UNSPECIFIED = 0;
  CPU = 1;
  MEMORY = 2;
  GPU_COMPUTE = 3;
  FLOOR_SEAT = 4;
  NETWORK_CAPABILITY = 5;
}

enum ResourceSource {
  RESOURCE_SOURCE_UNSPECIFIED = 0;
  DESIRED_GRAPH = 1;
  MANIFEST = 2;
  SANDBOX_FLOOR = 3;
  POLICY_FLOOR = 4;
}
```

These four payload-companion enums are themselves closed; together with the four headline enums in §3 (`GraphEvaluationResult`, `TransitionKind`, `DependencySolveResult`, `ABPromotionState`), this sub-spec contributes **eight closed enums** to `aios.sgr.v1alpha1`. None are open. None can be loosened by bundle.

## §14 Telemetry contract

The SGR exports the following Prometheus-compatible metrics. Cardinality budget per metric: ≤ 30 active label tuples on a typical host; ≤ 100 on a heavily-loaded host. Cardinality is bounded by the closed enums declared in §3 (no free-form labels are emitted).

| Metric                                 | Type      | Labels (closed)                                                                                                             |
| -------------------------------------- | --------- | --------------------------------------------------------------------------------------------------------------------------- |
| `sgr_graph_evaluation_total`           | counter   | `result` ∈ `GraphEvaluationResult` (5 values + `_UNSPECIFIED` excluded)                                                     |
| `sgr_graph_evaluation_duration_ms`     | histogram | `result` ∈ `GraphEvaluationResult` (buckets: 1, 2, 4, 8, 16, 32, 50, 100, 200, 500, 1000)                                   |
| `sgr_dependency_solve_total`           | counter   | `result` ∈ `DependencySolveResult` (4 values)                                                                               |
| `sgr_transition_total`                 | counter   | `kind` ∈ `TransitionKind` (10 values), `outcome` ∈ {`succeeded`, `failed`, `rolled_back`}                                   |
| `sgr_transition_duration_ms`           | histogram | `kind` ∈ `TransitionKind`, `outcome` ∈ {`succeeded`, `failed`}                                                              |
| `sgr_ab_promotion_state_total`         | counter   | `state` ∈ `ABPromotionState` (5 values), `transition` (closed list of 6 legal pairs from §3.4)                              |
| `sgr_transition_rate_limited_total`    | counter   | `bucket` ∈ {`subject`, `unit`, `ab_promote_global`}                                                                         |
| `sgr_transition_conflict_total`        | counter   | `conflict_kind` ∈ {`incompatible_kinds`, `overlapping_reconfigure`, `overlapping_ab`}                                       |
| `sgr_resource_budget_denied_total`     | counter   | `dimension` ∈ {`cpu`, `memory`, `gpu_compute`, `floor_seat`, `network_capability`}, `source` ∈ closed §6.1 list (4 sources) |
| `sgr_dependency_cycle_detected_total`  | counter   | none (rare event; FOREVER evidence sufficient for diagnostics)                                                              |
| `sgr_result_cache_hit_total`           | counter   | `result` ∈ `GraphEvaluationResult`                                                                                          |
| `sgr_result_cache_size`                | gauge     | none                                                                                                                        |
| `sgr_evaluation_budget_exceeded_total` | counter   | `outcome` ∈ {`failed`, `degraded`}                                                                                          |
| `sgr_transition_queue_depth`           | gauge     | `kind` ∈ `TransitionKind`                                                                                                   |
| `sgr_active_promotions`                | gauge     | `state` ∈ `ABPromotionState`                                                                                                |

The metrics complement the evidence log: metrics are for live operability dashboards (S15.5 admin tooling) and quick anomaly detection; evidence is the audit trail. The two never disagree on terminal counts: `sgr_transition_total{outcome="succeeded"}` equals the cardinality of `TRANSITION_SUCCEEDED` evidence records over the same window.

## §15 Golden fixtures

These fixtures are the acceptance harness for any SGR implementation. Each fixture is an end-to-end scenario specified by inputs, expected enum outcomes, and expected evidence records.

### §15.1 Fixture A — `CONVERGED` short-circuit

**Inputs:** `G_live` and `G_desired` canonicalize to the same hash. Floor and bundle unchanged.

**Expected:**

- `Evaluate()` returns within p50 budget.
- `GraphEvaluationResult = CONVERGED`.
- `TransitionPlan.transitions = []`.
- One `GRAPH_CONVERGED` record emitted (STANDARD_24M).
- No `TRANSITION_*` records emitted.
- Result cached.

### §15.2 Fixture B — Single `START` transition

**Inputs:** `G_live` empty for unit `u1`. `G_desired` declares `u1` running. All preconditions satisfied.

**Expected sequence of records:** `GRAPH_EVALUATED`, `TRANSITION_QUEUED{kind=START}`, `TRANSITION_STARTED`, `TRANSITION_SUCCEEDED`, `GRAPH_CONVERGED`.

### §15.3 Fixture C — A/B happy path

Identical to §11.1. Expected terminal records: `AB_CANARY_PROMOTED`, `TRANSITION_SUCCEEDED`, `GRAPH_CONVERGED`. No `AB_ROLLBACK_PERFORMED`.

### §15.4 Fixture D — A/B rollback (pre-promote)

**Inputs:** Variant B fails the first probe (failure_count = 1). Second probe also fails (failure_count = 2). State transitions `B → ROLLBACK`.

**Expected records:** `TRANSITION_QUEUED{kind=UPGRADE_AB_PROMOTE}`, `TRANSITION_STARTED`, `AB_ROLLBACK_PERFORMED` (FOREVER), `TRANSITION_FAILED{reason=AB_HEALTHCHECK_THRESHOLD}` (EXTENDED_60M). Variant A continues serving 100% throughout.

### §15.5 Fixture E — A/B rollback (canary)

**Inputs:** Variant B passes probe 1 (success_count = 1, state → `CANARY`, 5% traffic). Probe 2 fails (failure_count = 1). Probe 3 fails (failure_count = 2, state → `ROLLBACK`).

**Expected records:** Same as Fixture D plus the implicit `CANARY` state observed via `sgr_ab_promotion_state_total{state="CANARY"}` increment. No `AB_CANARY_PROMOTED`.

### §15.6 Fixture F — `BLOCKED_DEPENDENCY`

**Inputs:** Unit `app` requires unit `db` STABLE. `db` is currently in `B` state.

**Expected:** `GRAPH_EVALUATED` with `dep_result=WAITING`, no transitions dispatched. Repeated evaluations emit `GRAPH_EVALUATED` until `db` reaches STABLE; then `app` START dispatched.

### §15.7 Fixture G — `BLOCKED_RESOURCE`

**Inputs:** Desired graph requests memory exceeding sandbox-floor for the unit's class.

**Expected:** `RESOURCE_BUDGET_DENIED` (EXTENDED_60M), `GRAPH_BLOCKED_RESOURCE` (STANDARD_24M), no transitions dispatched. On capacity free (e.g., another unit STOPped), next evaluation proceeds.

### §15.8 Fixture H — Cycle detection

**Inputs:** Manifest declares `u1.requires = [u2]` and `u2.requires = [u1]`.

**Expected:** `DependencySolveResult = CYCLE`. `GraphEvaluationResult = FAILED`. `DEPENDENCY_CYCLE_DETECTED` (FOREVER) emitted with `cycle_nodes = [u1, u2]`. Result cached as FAILED. Re-submission of the same inputs returns the cached FAILED result without re-evaluation.

### §15.9 Fixture I — Conflict detection

Identical to §11.3. Expected terminal record: `TRANSITION_CONFLICT` (FOREVER) with `winning_transition_id`, `rejected_transition_id`, `conflict_kind = INCOMPATIBLE_KINDS`.

### §15.10 Fixture J — Rate limit (per-subject)

**Inputs:** Subject submits 31 transitions in 60 seconds.

**Expected:** First 30 dispatched normally. Transition 31 rejected with `TransitionRateLimited`. `TRANSITION_RATE_LIMITED` evidence (STANDARD_24M) emitted. `sgr_transition_rate_limited_total{bucket="subject"}` increments by 1.

### §15.11 Fixture K — Floor swap mid-evaluation

**Inputs:** Plan cached with floor signature `sigfloor_A`. Floor rotates to `sigfloor_B`. Same `(graph_state_hash, target_state_hash)` pair re-evaluated.

**Expected:** Cache key differs (floor signature is part of the key). Fresh evaluation. Different `transition_plan_id` if the new floor changes any `BLOCKED_RESOURCE` outcomes; identical otherwise. Cached entry under `sigfloor_A` is **not** consulted.

### §15.12 Fixture L — Idempotent re-evaluation

**Inputs:** Same `(graph_state_hash, target_state_hash, floor_signature, policy_bundle_hash)` pair, same `IN_PROGRESS` outcome, no in-flight transitions on relevant subjects.

**Expected:** `sgr_result_cache_hit_total` increments. Same `transition_plan_id` returned byte-identical. No fresh `GRAPH_EVALUATED` record emitted (cache-hit path is silent for STANDARD_24M records; only the metric increments). This guarantees that the operator polling the SGR does not flood the evidence log.

## §16 Acceptance criteria

- [ ] `GraphEvaluationResult` is a closed enum with exactly five values (plus the `_UNSPECIFIED` sentinel).
- [ ] `TransitionKind` is a closed enum with exactly ten values (plus the `_UNSPECIFIED` sentinel).
- [ ] `DependencySolveResult` is a closed enum with exactly four values (plus the `_UNSPECIFIED` sentinel).
- [ ] `ABPromotionState` is a closed enum with exactly five values (plus the `_UNSPECIFIED` sentinel).
- [ ] The graph evaluation algorithm is deterministic: same `(graph_state_hash, target_state_hash, floor_signature, policy_bundle_hash)` produces the same `transition_plan_id`.
- [ ] The result cache rejects re-evaluation of `FAILED` outcomes for the same input tuple.
- [ ] The N=3 successful health-check / N=2 failed health-check thresholds are hard-coded and not bundle-overridable.
- [ ] The probe cadence is manifest-overridable within `[5s, 60s]`.
- [ ] Resource budget composition uses most-restrictive-wins across (desired_graph, manifest, sandbox_floor, policy_floor) — four sources.
- [ ] Per-subject transition rate limits (30/60s, 5/60s per unit, 100/24h per `UPGRADE_AB_PROMOTE` global) are hard-coded.
- [ ] Concurrent contradictory transitions on the same `unit_id` are rejected with `TransitionConflict`.
- [ ] Cycles in the dependency graph cause `DependencySolveResult = CYCLE`, `GraphEvaluationResult = FAILED`, and FOREVER evidence; no heuristic edge-removal.
- [ ] Twelve evidence record types (§9) are queued for S3.1 with assigned retention classes.
- [ ] `AB_ROLLBACK_PERFORMED`, `DEPENDENCY_CYCLE_DETECTED`, and `TRANSITION_CONFLICT` are FOREVER-retained.
- [ ] Graph evaluation p95 < 50ms; budget exceedance emits `GRAPH_EVALUATION_BUDGET_EXCEEDED` (queued for S3.1 Wave 6).
- [ ] Per-`TransitionKind` performance budgets per §8.2 are documented for all ten kinds.
- [ ] All three worked examples (§11) trace cleanly through the FSM and emit the correct evidence chain.
- [ ] The "Consumes" header lists only `{L0, L1, L2, L3}` specs, conforming to `INV-007`.
- [ ] Every transition terminal state (success or failure) emits at least one evidence record, conforming to `INV-014`.

## See also

- [S15.1 — Unit Manifest](01_unit_manifest.md) (sibling sub-spec)
- [S10.1 — Capability Runtime gRPC](03_capability_runtime_grpc.md)
- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S2.4 — Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S3.2 — Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [S14.1 — Failure Handling](../L9_Observability_Admin_Operations/03_failure_handling.md) (pattern donor)
- [L0 — Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md) (`INV-007`, `INV-014`)
- [Rev.1 §10 — AIOS-SGR](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.1 §13 — Typed Actions and Capability Runtime](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
- [L3 Overview](00_overview.md)
