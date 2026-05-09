# AIOS-SGR Unit Manifest (Rev.2)

| Field          | Value                                                                                                                                                        |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists; structural contract complete)                                                                |
| Phase tag      | S15.1                                                                                                                                                        |
| Layer          | L3 AIOS-SGR / Service Graph Runtime                                                                                                                          |
| Schema package | `aios.unit.v1alpha1`                                                                                                                                         |
| Consumes       | S0.1 Action Envelope, S10.1 Capability Runtime gRPC, S3.2 Sandbox Composition, S2.4 Verification Grammar, S3.1 Evidence Log, S1.3 Object Model (versioning), |
|                | S11.1 Repository Model + Trust Roots (publisher signatures), S14.1 Failure Handling (`RESOURCE_EXHAUSTION`), L0 invariant catalog (INV-014, INV-017)         |
| Produces       | typed `UnitManifest` proto, closed `UnitKind` / `UnitState` / `RestartPolicy` / `HealthCheckKind` enums, dependency DAG semantics, eight evidence record     |
|                | types queued for next-Wave S3.1 consolidation, three worked examples                                                                                         |

## §1 Purpose

This sub-spec defines the **service unit manifest** consumed by the AIOS-SGR (Service Graph Runtime). The SGR owns the host's desired-state graph: every service, one-shot job, timer, mount, device, app session, agent worker, and model server is described by a typed `UnitManifest`, and the SGR drives the host so that the live state matches the desired state declared by the manifests.

The manifest is **the** declarative contract for what runs on an AIOS host. Adapters (per `04_adapter_model.md` — `SHELL`) execute typed actions; the Capability Runtime (per `03_capability_runtime_grpc.md` — `REAL`/E1) dispatches them. But the runtime decides _what_ should be running by walking the unit graph defined here.

A unit is not a free-form systemd unit file with arbitrary `ExecStart` lines. It is a closed-schema record signed by a publisher, validated against `aios.unit.v1alpha1`, hashed canonically, and pinned to a `rollback_pointer` AIOS-FS version per S1.3. Free-form shell as primary input is forbidden by L3 invariant (`00_overview.md` — "Adapters must not accept free-form shell commands as primary input") and INV-013 (AI cannot perform system admin operations).

This file defines:

1. The closed `UnitKind` enum (ten kinds covering the full service surface).
2. The closed `UnitState` FSM (eleven states, entry/exit transitions enumerated).
3. The closed `RestartPolicy` enum (five values).
4. The closed `HealthCheckKind` enum (five values).
5. The `UnitManifest` proto IDL with mandatory fields (`unit_id`, `unit_kind`, `dependencies`, `sandbox_profile_ref`, `verification_intent`, `rollback_pointer`, `resource_budget`, `restart_policy`, `health_check_kind`, `publisher_signature`).
6. The dependency DAG admission rule (acyclic, downward-only across layers).
7. The publisher signature discipline cited from S11.1.
8. The rollback-pointer discipline cited from S1.3.
9. The resource-budget binding to S14.1 `RESOURCE_EXHAUSTION`.
10. Eight evidence record types queued for S3.1 consolidation.
11. Adversarial-robustness rules (cyclic dependency, manifest forgery, resource exhaustion).
12. Three worked examples (system service, one-shot migration job, agent worker).

This file does **not** define:

- The graph evaluation algorithm or A/B promotion semantics. That is `02_state_transitions.md` (`SHELL`).
- The Capability Runtime gRPC surface. That is `03_capability_runtime_grpc.md` (S10.1, `REAL`).
- Adapter implementation patterns or per-adapter target schemas. That is `04_adapter_model.md` (`SHELL`).
- The sandbox profile composition algorithm. That is S3.2.
- The verification primitive vocabulary. That is S2.4.
- The evidence log hash chain or query API. That is S3.1.
- The publisher catalog admission flow or trust grading. That is S11.1.

## §2 Scope

In scope:

- The `UnitManifest` schema and signature discipline.
- The closed `UnitKind` enum (ten kinds).
- The closed `UnitState` FSM (eleven states) and the allowed-transition table.
- The closed `RestartPolicy` enum (five values).
- The closed `HealthCheckKind` enum (five values).
- The dependency DAG admission rule.
- The mandatory `sandbox_profile_ref` field bound to S3.2.
- The mandatory `verification_intent` field bound to S2.4.
- The mandatory `rollback_pointer` field bound to S1.3.
- The mandatory `resource_budget` field bound to S14.1.
- The mandatory `publisher_signature` field bound to S11.1.
- Eight evidence record types queued for S3.1.
- Three worked examples end-to-end.

Out of scope:

- Graph evaluation, dependency solve, A/B promotion (`02_state_transitions.md`).
- Per-adapter target schemas (`04_adapter_model.md`).
- The action envelope shape (S0.1).
- Sandbox profile composition (S3.2).
- Verification primitive grammar (S2.4).
- Evidence log mechanics (S3.1).
- Publisher onboarding and trust-grade flow (S11.1).
- Multi-host federation; Rev.2 assumes one authoritative SGR per host.

## §3 Vocabulary

This section declares the closed enums on which the rest of the sub-spec is built. Every enum is contract-grade. Adding a value is a versioned spec change. Bundle load fails on unknown values. Wire compatibility is governed by the S0.1 §8 versioning policy.

### §3.1 `UnitKind`

Closed enum, ten values. Every unit on an AIOS host belongs to exactly one kind. Adapters register against one or more kinds (per `04_adapter_model.md`); the runtime refuses to dispatch a unit whose kind has no registered adapter.

| Value           | Semantics                                                                                                                                                                                                                                      |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SERVICE`       | Long-running daemon. Has a startup contract, a health contract, and a stop contract. Default for system services and userland daemons. Cited in `02_state_transitions.md` as the default unit kind for graph nodes.                            |
| `ONE_SHOT_JOB`  | Single-execution job that runs to completion. No `RUNNING → HEALTHY` transition; transitions `STARTING → RUNNING → STOPPED` (success) or `STARTING → RUNNING → FAILED`. Used for migrations, one-time setup, recovery rehearsals.              |
| `TIMER`         | Periodic trigger that schedules a `ONE_SHOT_JOB` or sends a typed action at a declared cadence. The timer itself is `RUNNING`/`HEALTHY` while the scheduler is alive; the spawned jobs have their own unit lifecycle.                          |
| `MOUNT`         | A filesystem mount or AIOS-FS namespace projection. `HEALTHY` means the mount is reachable and matches the manifest's expected mount point and options. Required for AIOS-FS root projections per L2 namespace catalog.                        |
| `DEVICE`        | A hardware device binding (block device, network interface, GPU partition per L8.2, audio device). `HEALTHY` requires the device to be present and the driver to expose its declared capability surface.                                       |
| `APP_SESSION`   | A per-user app instance under L6 app runtime model (S12.1). The lifecycle is bounded by the user session; multiple `APP_SESSION` units may exist for the same package across sessions and groups.                                              |
| `AGENT_WORKER`  | A subject-bearing AI agent worker process per L5 cognitive core (S13.1). Subject `is_ai = true`. Cannot author CHROME-zone surfaces (INV-023). Cannot self-grade (INV-016).                                                                    |
| `MODEL_SERVER`  | A local LLM/embedding/inference server (Ollama, vLLM-compatible, or AIOS-native). Health includes load-status of the declared model artifact; `DEGRADED` if model is loaded but accuracy probes fail.                                          |
| `RECOVERY_TASK` | A unit that runs only in `is_recovery_mode = true` per INV-012. Normal-mode SGR refuses to start `RECOVERY_TASK` units; recovery-mode SGR ignores all non-recovery units.                                                                      |
| `OBSERVER`      | A passive observer (telemetry collector, audit reader, evidence streamer) that does not mutate state. Observers are restricted to `READ_ONLY` adapter capabilities; manifests declaring `OBSERVER` with mutating capabilities fail validation. |

The set is sealed. Adding an eleventh kind is a versioned spec change requiring a new `aios.unit.v1alphaN` package.

### §3.2 `UnitState`

Closed enum, eleven states. This is the per-unit FSM observed by the SGR; it is distinct from the per-action `ActionLifecycleState` of S10.1 (an action acts _on_ a unit; the unit has its own multi-action lifecycle).

| Value       | Phase semantics                                                                                                                                                                                 |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `DRAFT`     | Manifest accepted into the desired-state object store but not yet admitted into the active graph. Validation may still be in flight. Cannot be dispatched.                                      |
| `QUEUED`    | Admitted into the active graph; waiting for dependencies to become `HEALTHY` before transitioning to `STARTING`.                                                                                |
| `STARTING`  | Dependencies satisfied; an `ExecuteAction` of kind `unit.start` (per `04_adapter_model.md`) is in flight against the unit's adapter.                                                            |
| `RUNNING`   | Adapter reported successful start; the unit is live but the first health check has not yet passed. Default for `ONE_SHOT_JOB` mid-execution.                                                    |
| `HEALTHY`   | Health check passed under the declared `HealthCheckKind` (§3.4). Terminal-by-design for `ONE_SHOT_JOB` is `STOPPED`, not `HEALTHY`; `HEALTHY` is a steady state for `SERVICE`/`MOUNT`/`DEVICE`. |
| `DEGRADED`  | Health check passed in degraded mode (e.g. probe returned `WARNING`, or one of N replicas unhealthy). The unit is still serving but operator attention is recommended.                          |
| `UNHEALTHY` | Health check failed under the declared `HealthCheckKind`. Restart policy decides whether the SGR triggers `unit.restart` or transitions to `FAILED`.                                            |
| `STOPPING`  | An `ExecuteAction` of kind `unit.stop` is in flight. Adapter is performing graceful shutdown.                                                                                                   |
| `STOPPED`   | Unit has completed its lifecycle. For `ONE_SHOT_JOB`, this is the success terminal state. For `SERVICE`, this means the unit was deliberately stopped and is not eligible for auto-restart.     |
| `FAILED`    | Unit failed to reach `HEALTHY` within the manifest's `startup_deadline_seconds`, or `UNHEALTHY` exceeded the restart budget, or adapter reported terminal failure. Operator action required.    |
| `RETIRED`   | Manifest was withdrawn from the desired-state graph (operator action or publisher deplatform per S11.1). Forensic state; the unit is no longer eligible for any action.                         |

The eleven-state set is sealed.

#### §3.2.1 Allowed transitions

| From        | To                                   | Trigger                                                                      |
| ----------- | ------------------------------------ | ---------------------------------------------------------------------------- |
| `DRAFT`     | `QUEUED`                             | Admission validation succeeded                                               |
| `DRAFT`     | `RETIRED`                            | Validation failed; manifest withdrawn                                        |
| `QUEUED`    | `STARTING`                           | All declared dependencies are `HEALTHY`                                      |
| `QUEUED`    | `RETIRED`                            | Operator withdraws or dependency permanently unsatisfiable                   |
| `STARTING`  | `RUNNING`                            | Adapter `unit.start` returned success                                        |
| `STARTING`  | `FAILED`                             | Adapter timed out / refused / crashed                                        |
| `RUNNING`   | `HEALTHY`                            | First health check passed                                                    |
| `RUNNING`   | `STOPPED`                            | `ONE_SHOT_JOB` completed successfully                                        |
| `RUNNING`   | `FAILED`                             | `ONE_SHOT_JOB` exited non-zero, or `startup_deadline_seconds` expired        |
| `HEALTHY`   | `DEGRADED`                           | Health check returned `WARNING` or partial unhealthy                         |
| `HEALTHY`   | `UNHEALTHY`                          | Health check failed                                                          |
| `HEALTHY`   | `STOPPING`                           | Operator stop, dependency stop, or rollback triggered                        |
| `DEGRADED`  | `HEALTHY`                            | Health check recovered                                                       |
| `DEGRADED`  | `UNHEALTHY`                          | Degradation worsened                                                         |
| `DEGRADED`  | `STOPPING`                           | Operator stop or rollback triggered                                          |
| `UNHEALTHY` | `HEALTHY`                            | Restart succeeded and health check recovered                                 |
| `UNHEALTHY` | `STARTING`                           | Restart policy admits a new attempt                                          |
| `UNHEALTHY` | `FAILED`                             | Restart budget exhausted                                                     |
| `UNHEALTHY` | `STOPPING`                           | Operator stop                                                                |
| `STOPPING`  | `STOPPED`                            | Adapter `unit.stop` returned success                                         |
| `STOPPING`  | `FAILED`                             | Adapter `unit.stop` failed (rollback semantics in `02_state_transitions.md`) |
| `STOPPED`   | `QUEUED`                             | Operator restart / scheduled restart per `RestartPolicy = SCHEDULED`         |
| `STOPPED`   | `RETIRED`                            | Manifest withdrawn                                                           |
| `FAILED`    | `STARTING`                           | Operator-initiated retry (manual)                                            |
| `FAILED`    | `RETIRED`                            | Operator gives up; manifest withdrawn                                        |
| `RETIRED`   | (terminal — no outgoing transitions) | —                                                                            |

Any transition not in the table is forbidden. The SGR rejects internal callers that attempt a forbidden transition with `IllegalUnitStateTransition` and emits a `TAMPER_DETECTED` evidence record per INV-005.

### §3.3 `RestartPolicy`

Closed enum, five values. The SGR consults this enum on every `UNHEALTHY → ?` and `STOPPING → STOPPED` transition.

| Value            | Semantics                                                                                                                                                      |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `NEVER`          | Unit never restarts. On `UNHEALTHY` or `RUNNING → FAILED`, the SGR transitions directly to `FAILED`. Default for `ONE_SHOT_JOB` and `RECOVERY_TASK`.           |
| `ON_FAILURE`     | Restart only on `UNHEALTHY` or unexpected `FAILED`. A clean `STOPPED` (operator stop, success exit) does not trigger restart. Default for `SERVICE`.           |
| `ALWAYS`         | Restart on every exit, including clean `STOPPED`. Used for daemons that must run continuously (e.g. core platform agents). Restart budget still applies.       |
| `UNLESS_STOPPED` | Restart on every exit unless the operator explicitly issued `unit.stop`. Mirrors the docker `unless-stopped` semantic. Used for app sessions and most daemons. |
| `SCHEDULED`      | Restart only on the cadence declared by an associated `TIMER` unit. Used for periodic jobs that should not restart on failure (next tick handles it).          |

The five-value set is sealed. Adding a sixth (e.g. `EXPONENTIAL_BACKOFF`) is a versioned spec change.

#### §3.3.1 Restart budget

For policies `ON_FAILURE` / `ALWAYS` / `UNLESS_STOPPED`, the manifest may declare:

```proto
message RestartBudget {
  uint32 max_attempts            = 1;  // default 5
  uint32 reset_window_seconds    = 2;  // default 300 (5 min)
  uint32 backoff_initial_seconds = 3;  // default 2
  uint32 backoff_max_seconds     = 4;  // default 60
}
```

When `max_attempts` is exhausted within `reset_window_seconds`, the SGR transitions the unit to `FAILED`, emits `UNIT_FAILED` (§7), and the failure is classified as S14.1 `COMPONENT_RESTART_BUDGET_EXHAUSTED` per S14.1 §4.1 row 25/27.

### §3.4 `HealthCheckKind`

Closed enum, five values. Every unit (except `OBSERVER`, which has no health surface beyond liveness) declares exactly one `HealthCheckKind`. Free-form shell probes are forbidden as primary input per L3 invariant.

| Value                    | Semantics                                                                                                                                                                                                                                                                                           |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `TCP_PORT`               | The unit declares a TCP port; the SGR connects, expects connection acceptance within `probe_timeout_seconds`, then closes. Suitable for `SERVICE` units that listen on a stable port.                                                                                                               |
| `HTTP_OK`                | The SGR issues an HTTP GET to a declared path (default `/healthz`); expects 2xx within `probe_timeout_seconds`. Suitable for `SERVICE` and `MODEL_SERVER` units that expose an HTTP health endpoint.                                                                                                |
| `COMMAND_EXIT_ZERO`      | The SGR dispatches a typed `unit.health_probe` action to the unit's adapter; the adapter runs a closed-schema probe (no free-form shell) and reports exit zero on success. The probe's parameters live in `health_check_args` (typed proto, not shell argv).                                        |
| `AIOSFS_POINTER_HEALTHY` | The SGR resolves an AIOS-FS pointer (per S1.3 `Pointer.current_version_id`) and confirms the version is not in `QUARANTINE` and not `RETIRED`. Suitable for `MOUNT` units and for units whose health is gated by an AIOS-FS object's version being live.                                            |
| `CUSTOM_PROBE`           | A registered probe published under the unit's adapter manifest with a closed parameter schema. `CUSTOM_PROBE` is **not** a free-form escape hatch; the probe must be registered with a typed schema and signed by the publisher (S11.1). Used for hardware probes and model-server accuracy probes. |

The five-value set is sealed.

#### §3.4.1 Probe cadence and grace

```proto
message HealthCheckSpec {
  HealthCheckKind kind            = 1;
  uint32 probe_interval_seconds   = 2;   // default 10
  uint32 probe_timeout_seconds    = 3;   // default 3
  uint32 startup_grace_seconds    = 4;   // window after STARTING during which probe failures don't count
  uint32 unhealthy_threshold      = 5;   // consecutive failures to transition HEALTHY → UNHEALTHY (default 3)
  uint32 healthy_threshold        = 6;   // consecutive successes to transition UNHEALTHY → HEALTHY (default 2)
  google.protobuf.Struct args     = 7;   // typed args, validated against the kind's schema
}
```

`startup_grace_seconds` is essential: it prevents the SGR from flapping a unit into `UNHEALTHY` during the legitimate startup window (e.g. a database server warming caches). The grace window is bounded by the unit's `startup_deadline_seconds`.

## §4 The `UnitManifest` proto

```proto
syntax = "proto3";
package aios.unit.v1alpha1;

import "google/protobuf/struct.proto";
import "google/protobuf/timestamp.proto";

message UnitManifest {
  string schema_version             = 1;   // "aios.unit.v1alpha1"
  string unit_id                    = 2;   // "unit:<vendor>:<name>" — see §4.1
  UnitKind unit_kind                = 3;
  string display_name               = 4;   // human-readable; ≤256 chars
  string description                = 5;   // ≤2048 chars
  google.protobuf.Timestamp issued_at = 6;
  string publisher_id               = 7;   // "pub_<ULID>" per S11.1
  string publisher_root_id          = 8;   // S11.1 publisher root key id
  bytes  publisher_signature        = 9;   // Ed25519 over canonical bytes (§4.2)
  string canonical_hash             = 10;  // hex_lower(BLAKE3(jcs(this.without_signature)))[:32]

  // Dependency graph (§5)
  repeated UnitDependency dependencies = 11;

  // Sandbox binding (§6)
  string sandbox_profile_ref        = 12;  // "prof_<hex>" per S3.2

  // Verification (§7)
  repeated VerificationIntentRef verification_intent = 13;

  // Rollback (§8)
  RollbackPointer rollback_pointer  = 14;

  // Resource budget (§9)
  ResourceBudget resource_budget    = 15;

  // Restart and health (§3.3, §3.4)
  RestartPolicy restart_policy      = 16;
  RestartBudget restart_budget      = 17;
  HealthCheckSpec health_check      = 18;

  // Lifecycle parameters
  uint32 startup_deadline_seconds   = 19;  // bound for STARTING → RUNNING
  uint32 stop_deadline_seconds      = 20;  // bound for STOPPING → STOPPED
  google.protobuf.Struct adapter_target = 21;  // typed target schema per `04_adapter_model.md`

  // Optional
  google.protobuf.Struct labels     = 22;  // free-form metadata; bounded ≤4 KiB serialized
  string correlation_id             = 23;  // for batch admission grouping
}

message UnitDependency {
  string unit_id                    = 1;
  DependencyKind kind               = 2;
}

enum DependencyKind {
  DEPENDENCY_KIND_UNSPECIFIED = 0;
  REQUIRES_HEALTHY            = 1;  // dependency must be HEALTHY before this unit can transition QUEUED → STARTING
  REQUIRES_RUNNING            = 2;  // dependency must be RUNNING (HEALTHY or DEGRADED both acceptable)
  ORDERS_AFTER                = 3;  // soft ordering: if dependency exists, this unit starts after; absence is not a block
}

message VerificationIntentRef {
  // Wire-compatible with S0.1 `VerificationIntent { type, args }` and S2.4 §3 IDL.
  string type                       = 1;
  google.protobuf.Struct args       = 2;
}

message RollbackPointer {
  string aiosfs_pointer_id          = 1;  // S1.3 pointer "ptr_<ULID>" or path projection
  string expected_current_version_id = 2; // S1.3 §3.6 CAS expectation
  RollbackTrigger trigger           = 3;
}

enum RollbackTrigger {
  ROLLBACK_TRIGGER_UNSPECIFIED = 0;
  ON_STARTUP_FAILURE           = 1;  // STARTING → FAILED triggers rollback
  ON_HEALTH_FAILURE            = 2;  // UNHEALTHY transition with budget exhausted triggers rollback
  ON_OPERATOR_REQUEST          = 3;  // explicit operator action only
  NEVER                        = 4;  // unit has no rollback (e.g. truly idempotent ONE_SHOT_JOB)
}

message ResourceBudget {
  uint64 memory_bytes_max           = 1;
  double cpu_quota_cores            = 2;  // fractional cores; 1.0 = one core
  uint64 disk_bytes_max             = 3;  // ephemeral disk usage cap
  uint32 file_descriptors_max       = 4;
  uint32 process_count_max          = 5;
  uint32 queue_depth_max            = 6;  // for AGENT_WORKER and MODEL_SERVER inbound queue
  GpuBudget gpu                     = 7;  // optional; INV-024
}

message GpuBudget {
  bool requires_compute             = 1;  // when true, INV-024 requires gpu.compute_heavy capability
  uint64 vram_bytes_max             = 2;
}
```

### §4.1 `unit_id` format

```text
unit:<vendor>:<name>[:<variant>]
```

- `<vendor>` lowercase DNS-segment, ≤63 chars; controlled by the publisher and bound to `publisher_root_id`.
- `<name>` lowercase ASCII alphanumeric + `_` + `-`, ≤63 chars.
- `<variant>` optional; same charset; used to disambiguate per-environment forks.

Examples:

- `unit:aios:capability_runtime`
- `unit:aios:evidence_log`
- `unit:user-247:agent_worker:rust-coder`
- `unit:proxguard:gateway:lan`

The SGR rejects unit IDs whose `<vendor>` segment does not appear in the AIOS-root-signed publisher catalog (`pubcat_<hex>` per S11.1). Cross-vendor name collisions are forbidden by construction (the vendor segment namespaces the rest).

### §4.2 Canonical hashing and signing

The manifest is hashed under the S0.1 §8 canonical proto rules:

1. Strip `publisher_signature` and `canonical_hash` from the message.
2. Encode with deterministic proto serialization (S0.1 §8.2).
3. Compute `BLAKE3-256` over the bytes; canonical hash = first 32 hex chars (128 bits).
4. The publisher signs the **full** 256-bit hash (not the truncated `canonical_hash` field) with their `publisher_root_id`-anchored Ed25519 key per S11.1.
5. The manifest is admitted if the signature verifies and the publisher's trust level (per S11.1 `PublisherTrustLevel`) admits the unit's kind.

A signature failure or trust-level mismatch produces `MANIFEST_FORGED` evidence (FOREVER retention per S11.1) and the manifest is rejected at admission. INV-014 ("no proof, no completion") makes this mechanical: a manifest without a valid signature cannot be `REAL`-status in the desired-state graph.

## §5 Dependency DAG

The `dependencies` field is a list of `UnitDependency` records. Together they form a directed graph over the unit set on the host.

### §5.1 Acyclicity

The graph **must be acyclic**. The SGR's admission validator runs DFS over the graph at every manifest admission and rejects any submission that would close a cycle. The validator emits `UNIT_DEPENDENCY_CYCLE_DETECTED` (§7) and refuses to transition the unit out of `DRAFT`.

The cycle check considers `REQUIRES_HEALTHY` and `REQUIRES_RUNNING` edges as hard edges; `ORDERS_AFTER` edges are still part of the graph for cycle detection (a soft-ordering cycle is still a cycle in operational terms).

### §5.2 Layer-downward dependency

INV-007 ("layers depend downward only") applies to units exactly as it applies to specs. A unit that operates at conceptual layer L_n may depend on units at layers ≤ L_n. The SGR rejects manifests whose declared dependency would invert the dependency stack (e.g. an L1 mount unit depending on an L5 agent worker is rejected).

The layer of a unit is inferred from its `unit_kind` and its publisher's manifested layer scope:

| `unit_kind`     | Conceptual layer                    |
| --------------- | ----------------------------------- |
| `MOUNT`         | L1 / L2                             |
| `DEVICE`        | L1 / L8                             |
| `SERVICE`       | depends on publisher scope (L3..L9) |
| `ONE_SHOT_JOB`  | depends on publisher scope          |
| `TIMER`         | depends on publisher scope          |
| `APP_SESSION`   | L6                                  |
| `AGENT_WORKER`  | L5                                  |
| `MODEL_SERVER`  | L5                                  |
| `RECOVERY_TASK` | L1 / L9                             |
| `OBSERVER`      | L9                                  |

Cross-layer admission is computed at the publisher-scope level (per S11.1 publisher catalog declaration of allowed layers).

### §5.3 Dependency starvation

If a `REQUIRES_HEALTHY` dependency never reaches `HEALTHY` within the unit's `startup_deadline_seconds` after admission, the unit transitions `QUEUED → DRAFT → RETIRED` (no `STARTING` ever attempted). The SGR emits `UNIT_FAILED` with `ExecutionFailureReason = DEPENDENCY_UNREADY` (S10.1 §3.6) and links to the un-`HEALTHY` dependency.

## §6 Sandbox binding (S3.2)

The `sandbox_profile_ref` field references a composed sandbox profile by id (`prof_<hex>` per S3.2). The SGR consults the S3.2 `ComposeProfile` interface to materialize the actual profile at `STARTING` time; the manifest holds the **id** (post-composition reference), not the inline profile.

INV-017 ("sandbox floor is constitutional") applies. The composed profile is subject to the runtime safety floor: the unit cannot be started under a profile that loosens the constitutional floor. A manifest declaring a `sandbox_profile_ref` that loosens the floor is rejected at admission with `SANDBOX_FLOOR_VIOLATION` evidence.

For `AGENT_WORKER` units (`is_ai = true` subjects), the SGR additionally enforces the AI-class floor per S3.2 §5.4. For `RECOVERY_TASK` units, the recovery-class floor applies regardless of the manifest's declaration.

## §7 Verification binding (S2.4)

The `verification_intent` field is a list of typed verification intents per S2.4. Each entry is structurally identical to the S0.1 `VerificationIntent` shape (`type`, `args`).

The SGR runs verification on every transition into `HEALTHY`, `DEGRADED`, and `UNHEALTHY`. Verification failure transitions the unit to `UNHEALTHY` and emits `UNIT_DEGRADED` or `UNIT_UNHEALTHY` (§10) with the verification result attached as evidence per S3.1.

A manifest with an empty `verification_intent` list is rejected at admission unless `unit_kind ∈ {OBSERVER}`; observers are intrinsically passive and may declare zero intents. The rejection emits `MANIFEST_VALIDATION_REJECTED` and cites INV-014: a unit that cannot be verified cannot be `REAL`.

## §8 Rollback pointer (S1.3)

The `rollback_pointer` field binds the unit to an AIOS-FS object's version. When the unit transitions into a state matching the configured `RollbackTrigger`, the SGR issues a `unit.rollback` typed action (per `04_adapter_model.md`) that performs an S1.3 `PromotePointer` CAS with `expected_current_version_id` matching the manifest. CAS success rolls the pointer back to the prior version; CAS failure emits `UNIT_ROLLBACK_TRIGGERED` with `RollbackOutcome = CAS_FAILED` (S10.1 §3.7) and the unit transitions to `FAILED` with operator alert.

This binding is the mechanical realization of the L3 layer invariant "unsupported actions fail closed": a rollback that cannot be performed against the declared pointer is treated as a hard failure. The pointer is the source of truth, not the unit's runtime state.

For `ONE_SHOT_JOB` units that are intrinsically idempotent (e.g. `aiosfs.gc.run`), `RollbackTrigger = NEVER` is permitted; the manifest then declares `rollback_pointer.aiosfs_pointer_id = ""`.

## §9 Resource budget (S14.1)

The `resource_budget` field declares hard upper bounds on the unit's resource consumption. The SGR enforces these bounds via the sandbox profile (S3.2 backend probes) and observability layer (S14.1 telemetry). Crossing a bound triggers:

| Bound exceeded         | Behavior                                                                                                              |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `memory_bytes_max`     | Adapter is sent SIGTERM; on `stop_deadline_seconds` expiry, SIGKILL. Unit transitions `HEALTHY → UNHEALTHY → FAILED`. |
| `cpu_quota_cores`      | cgroup throttling; unit transitions to `DEGRADED` if throttled > 50% over `probe_interval_seconds × 3`.               |
| `disk_bytes_max`       | Adapter receives `DISK_BUDGET_EXCEEDED` event; unit transitions to `UNHEALTHY`.                                       |
| `file_descriptors_max` | New `open()` calls fail; unit transitions to `UNHEALTHY` after `unhealthy_threshold` consecutive failures.            |
| `process_count_max`    | New `fork()` calls fail; unit transitions to `UNHEALTHY`.                                                             |
| `queue_depth_max`      | Inbound queue rejects new entries; unit transitions to `DEGRADED`; if sustained, → `UNHEALTHY`.                       |
| `gpu.vram_bytes_max`   | Allocation fails; unit transitions to `UNHEALTHY` (subject to INV-024 capability gating for `requires_compute`).      |

Every budget exhaustion event is an instance of S14.1 `RESOURCE_EXHAUSTION` (S14.1 §3.1, value 12). The behavior table at S14.1 §4.1 rows 24–26 governs the layer-context mapping. The unit-level evidence record (`UNIT_DEGRADED` or `UNIT_FAILED`) carries `failure_class = RESOURCE_EXHAUSTION` and the specific bound that was crossed.

A manifest that declares zero or absent `resource_budget` is rejected at admission with `MANIFEST_VALIDATION_REJECTED`. The runtime cannot enforce bounds it does not have.

## §10 Evidence record types (queued for S3.1)

This sub-spec produces eight new evidence record types. They are queued for the next-Wave consolidation into S3.1's closed `RecordType` enum. Until then, they are reserved-name placeholders in this contract.

| RecordType                       | Retention class | When emitted                                                                                                                                                                                     |
| -------------------------------- | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `UNIT_REGISTERED`                | STANDARD_24M    | Manifest admitted; transition `DRAFT → QUEUED`. Carries `unit_id`, `unit_kind`, `canonical_hash`, `publisher_id`.                                                                                |
| `UNIT_STARTED`                   | STANDARD_24M    | Transition `STARTING → RUNNING`. Carries `unit_id`, adapter id, dispatch kind (per S10.1 §3.2), action_id of the `unit.start` envelope.                                                          |
| `UNIT_HEALTHY`                   | STANDARD_24M    | First transition into `HEALTHY` after `RUNNING` or recovery transition `UNHEALTHY → HEALTHY`. Carries `unit_id`, the verification result hashes (per S2.4 §11).                                  |
| `UNIT_DEGRADED`                  | EXTENDED_60M    | Transition `HEALTHY → DEGRADED`. Carries `unit_id`, the verification primitive that returned `WARNING`, and the operator-runbook reference (per S14.1 §10).                                      |
| `UNIT_FAILED`                    | EXTENDED_60M    | Transition into `FAILED` from any state. Carries `unit_id`, last `UnitState`, `ExecutionFailureReason` (S10.1 §3.6), and the linked action_id chain.                                             |
| `UNIT_STOPPED`                   | STANDARD_24M    | Transition `STOPPING → STOPPED` (clean stop). Carries `unit_id`, stop reason (`OPERATOR`, `DEPENDENCY_STOP`, `ROLLBACK`, `RETIREMENT`).                                                          |
| `UNIT_ROLLBACK_TRIGGERED`        | EXTENDED_60M    | A `RollbackTrigger` fired and `unit.rollback` was dispatched. Carries `unit_id`, `rollback_pointer.aiosfs_pointer_id`, the AIOS-FS CAS outcome, and the prior/next version_id pair.              |
| `UNIT_DEPENDENCY_CYCLE_DETECTED` | FOREVER         | Admission validator detected a cycle. Carries the unit_id under admission, the cycle path, and the publisher_id. FOREVER retention because cycle attempts may be adversarial-manifest forensics. |

`MANIFEST_FORGED` (S11.1, FOREVER) and `MANIFEST_VALIDATION_REJECTED` (queued narrative-only against this sub-spec, EXTENDED_60M) cover signature failure and validation rejection respectively; they are emitted by S11.1 trust-chain checks and S3.2 floor checks rather than by the SGR per se.

## §11 Adversarial robustness

### §11.1 Cyclic dependency

A manifest declaring a `REQUIRES_HEALTHY` edge that would close a cycle in the live graph is rejected at admission. The DFS validator runs in O(V + E) per admission; the worst case is bounded by the host's unit count (Rev.2 assumes ≤ 4096 active units per host).

The validator distinguishes between:

- **direct cycle** (A → B → A): rejected; `UNIT_DEPENDENCY_CYCLE_DETECTED` emitted with the two-element cycle path.
- **transitive cycle** (A → B → C → A): rejected; full path recorded.
- **self-loop** (A → A): rejected; `unit_id` recorded once with the loop annotation.

The cycle check is an L0 invariant-equivalent property at the SGR layer; it is not loosenable by any policy bundle (a bundle cannot disable cycle detection).

### §11.2 Manifest forgery

The publisher signature discipline (§4.2) is the structural defense. Three concrete attack patterns and their handling:

1. **Unsigned manifest:** missing `publisher_signature` → admission validator returns `MANIFEST_VALIDATION_REJECTED`; unit never leaves `DRAFT`.
2. **Wrong signing key:** the signature verifies against a key not in the publisher catalog `pubcat_<hex>` → S11.1 trust-chain check fires `TRUST_CHAIN_BROKEN`; manifest rejected; FOREVER evidence.
3. **Tampered hash:** the manifest's `canonical_hash` does not match a recomputation over the message bytes → manifest rejected with `MANIFEST_FORGED` (FOREVER) per S11.1.

For all three patterns, the AI subject path is hard-denied additionally by INV-002 (AI cannot execute) and INV-013 (AI cannot perform system admin operations) — only operator-class subjects may admit unit manifests touching `/aios/system/units/...`. AI subjects may **propose** a manifest into `/aios/agents/.../staging/` but cannot promote it into the active graph.

### §11.3 Resource budget exhaustion

Per §9, every bound exceedance maps to S14.1 `RESOURCE_EXHAUSTION` (FailureClass 12). Three concrete patterns:

1. **Memory leak in `SERVICE`:** `memory_bytes_max` exceeded → SIGTERM → `UNHEALTHY` → restart per `RestartPolicy`. If `max_attempts` exhausted, → `FAILED` and S14.1 row 25 (`COMPONENT_RESTART_BUDGET_EXHAUSTED`, FOREVER) fires.
2. **Adversarial queue flood on `AGENT_WORKER`:** `queue_depth_max` exceeded → backpressure (reject new) → `DEGRADED` → S14.1 row 26 (`CIRCUIT_BREAKER_OPENED`, EXTENDED_60M).
3. **Disk-full during `ONE_SHOT_JOB`:** `disk_bytes_max` exceeded → adapter returns `RESOURCE_BUDGET_EXCEEDED` (S10.1 §3.6) → unit transitions `RUNNING → FAILED`; rollback pointer fires per `RollbackTrigger.ON_STARTUP_FAILURE`; S14.1 row 24 (`READ_ONLY` degradation if pool-wide).

The budget is a constitutional cap, not an advisory limit. A unit cannot consume more than its declared bound; the sandbox floor (INV-017) enforces it mechanically.

### §11.4 Replay of retired manifests

A publisher that replays a `RETIRED` manifest's `canonical_hash` to attempt re-admission is rejected. The SGR maintains an L9-evidence-anchored set of retired hashes (per S3.1 query); any admission attempt with a hash in the retired set is rejected with `UNIT_REPLAY_REJECTED` (queued narrative-only, FOREVER). Re-admission of a previously-retired unit requires a new `canonical_hash` (i.e. a new `issued_at` and any other-field change), per the S0.1 §8.5 truncation rule.

### §11.5 Trust level downgrade between admission and start

If a publisher's trust level is downgraded (e.g. `VERIFIED → DEPRECATED` per S11.1) between manifest admission and the unit's `STARTING` transition, the SGR re-checks the trust chain at each transition. A trust level that no longer admits the unit's kind transitions the unit `QUEUED → RETIRED` and emits `UNIT_PUBLISHER_TRUST_REVOKED` (queued narrative-only, FOREVER, mirrored against S11.1's `PUBLISHER_DEPLATFORMED` family).

## §12 Worked examples

### §12.1 Example A — system service (Capability Runtime itself)

```yaml
# Conceptual JCS-like projection of the proto manifest.
schema_version: aios.unit.v1alpha1
unit_id: unit:aios:capability_runtime
unit_kind: SERVICE
display_name: AIOS Capability Runtime
description: |
  Internal orchestration RPC service per S10.1; dispatches typed actions to adapters.
issued_at: 2026-05-09T00:00:00Z
publisher_id: pub_01HXY9ROOTAIOS01KEY
publisher_root_id: aios-root
publisher_signature: "<Ed25519 over canonical bytes>"
canonical_hash: "a3f1c9e2..."

dependencies:
  - { unit_id: unit:aios:evidence_log, kind: REQUIRES_HEALTHY }
  - { unit_id: unit:aios:policy_kernel, kind: REQUIRES_HEALTHY }
  - { unit_id: unit:aios:vault_broker, kind: REQUIRES_RUNNING }
  - { unit_id: unit:aios:identity_service, kind: REQUIRES_HEALTHY }
  - { unit_id: unit:aios:aiosfs, kind: REQUIRES_HEALTHY }

sandbox_profile_ref: "prof_aios_runtime_floor_001"

verification_intent:
  - type: service.active
    args: { service: "aios-capability-runtime" }
  - type: port_open
    args: { host: "127.0.0.1", port: 7421 }
  - type: grpc_method_responsive
    args:
      {
        service: "aios.runtime.v1alpha1.CapabilityRuntime",
        method: "GetRuntimeInfo",
      }

rollback_pointer:
  aiosfs_pointer_id: "ptr_system_capability_runtime_release"
  expected_current_version_id: "ver_01HXY8K2..."
  trigger: ON_HEALTH_FAILURE

resource_budget:
  memory_bytes_max: 2147483648 # 2 GiB
  cpu_quota_cores: 2.0
  disk_bytes_max: 4294967296 # 4 GiB ephemeral
  file_descriptors_max: 16384
  process_count_max: 256
  queue_depth_max: 4096
  # gpu omitted; INV-024 not relevant.

restart_policy: ALWAYS
restart_budget:
  max_attempts: 5
  reset_window_seconds: 300
  backoff_initial_seconds: 2
  backoff_max_seconds: 60

health_check:
  kind: HTTP_OK
  probe_interval_seconds: 10
  probe_timeout_seconds: 3
  startup_grace_seconds: 30
  unhealthy_threshold: 3
  healthy_threshold: 2
  args: { path: "/healthz", scheme: "http", host: "127.0.0.1", port: 7421 }

startup_deadline_seconds: 60
stop_deadline_seconds: 30
adapter_target:
  binary_pointer: "ptr_system_capability_runtime_bin"
  args: ["--config", "/aios/system/runtime/config.toml"]

labels: { layer: "L3", criticality: "critical" }
correlation_id: corr_aios_bootstrap_001
```

Lifecycle walk:

1. SGR admits the manifest (signature OK, no cycle, dependencies known, sandbox floor respected). State = `DRAFT → QUEUED`. Evidence: `UNIT_REGISTERED`.
2. Dependencies reach `HEALTHY`. State = `QUEUED → STARTING`. SGR dispatches `unit.start` typed action through S10.1.
3. Adapter starts the binary. State = `STARTING → RUNNING`. Evidence: `UNIT_STARTED`.
4. After `startup_grace_seconds`, health probe `HTTP_OK /healthz` returns 200. State = `RUNNING → HEALTHY`. Evidence: `UNIT_HEALTHY`.
5. Steady state. Probe every 10 s; verification_intent re-evaluated per S2.4.

### §12.2 Example B — one-shot migration job

```yaml
schema_version: aios.unit.v1alpha1
unit_id: unit:aios:aiosfs_schema_migration_v2_to_v3
unit_kind: ONE_SHOT_JOB
display_name: AIOS-FS Schema Migration v2 → v3
description: |
  Migrates the on-disk metadata catalog from schema v2 to v3. Idempotent under retry.
issued_at: 2026-05-09T01:00:00Z
publisher_id: pub_01HXY9ROOTAIOS01KEY
publisher_root_id: aios-root
publisher_signature: "<Ed25519 over canonical bytes>"
canonical_hash: "b7a2f4d9..."

dependencies:
  - { unit_id: unit:aios:aiosfs, kind: REQUIRES_HEALTHY }

sandbox_profile_ref: "prof_aios_migration_floor_001"

verification_intent:
  - type: aiosfs_schema_version
    args: { expected: "v3" }
  - type: aiosfs_pointer_healthy
    args: { pointer: "ptr_system_aiosfs_metadata" }

rollback_pointer:
  aiosfs_pointer_id: "ptr_system_aiosfs_metadata"
  expected_current_version_id: "ver_01HXYM4..."
  trigger: ON_STARTUP_FAILURE

resource_budget:
  memory_bytes_max: 1073741824 # 1 GiB
  cpu_quota_cores: 1.0
  disk_bytes_max: 17179869184 # 16 GiB working set
  file_descriptors_max: 4096
  process_count_max: 32
  queue_depth_max: 0 # not applicable; one-shot

restart_policy: NEVER
restart_budget:
  {
    max_attempts: 0,
    reset_window_seconds: 0,
    backoff_initial_seconds: 0,
    backoff_max_seconds: 0,
  }

health_check:
  kind: COMMAND_EXIT_ZERO
  probe_interval_seconds: 0 # ignored; one-shot
  probe_timeout_seconds: 60
  startup_grace_seconds: 0
  unhealthy_threshold: 1
  healthy_threshold: 1
  args: { probe_id: "aiosfs.schema.is_v3" }

startup_deadline_seconds: 1800 # 30 min upper bound
stop_deadline_seconds: 60
adapter_target:
  migration_id: "schema_v2_to_v3_001"

labels: { layer: "L2", criticality: "critical", phase: "migration" }
correlation_id: corr_aiosfs_migration_v3_rollout
```

Lifecycle walk:

1. SGR admits. State = `DRAFT → QUEUED`. Evidence: `UNIT_REGISTERED`.
2. Dependency healthy. State = `QUEUED → STARTING`. Evidence: queued for `UNIT_STARTED`.
3. Adapter executes migration. State = `STARTING → RUNNING`.
4. Migration completes with exit zero; verification confirms `aiosfs_schema_version == v3`. State = `RUNNING → STOPPED`. Evidence: `UNIT_STOPPED` with reason `OPERATOR` (the migration was operator-initiated).
5. **Failure path:** if migration exit != 0, state = `RUNNING → FAILED` (RestartPolicy `NEVER`). `RollbackTrigger.ON_STARTUP_FAILURE` fires; `unit.rollback` is dispatched; AIOS-FS CAS attempts to roll the metadata pointer back to `expected_current_version_id`. CAS success → `UNIT_ROLLBACK_TRIGGERED` with `RollbackOutcome = SUCCEEDED`; CAS failure → `UNIT_ROLLBACK_TRIGGERED` with `RollbackOutcome = CAS_FAILED` and operator alert.

### §12.3 Example C — agent worker (AI subject)

```yaml
schema_version: aios.unit.v1alpha1
unit_id: unit:user-247:agent_worker:rust-coder
unit_kind: AGENT_WORKER
display_name: Rust Coder Agent (user-247)
description: |
  AI agent worker for Rust development assistance, scoped to user-247's group.
issued_at: 2026-05-09T02:00:00Z
publisher_id: pub_01HXZ9PUB_AGENT_VENDOR
publisher_root_id: pubroot_agent_vendor_v3
publisher_signature: "<Ed25519 over canonical bytes>"
canonical_hash: "c4e7a1b8..."

dependencies:
  - { unit_id: unit:aios:capability_runtime, kind: REQUIRES_HEALTHY }
  - { unit_id: unit:aios:identity_service, kind: REQUIRES_HEALTHY }
  - { unit_id: unit:aios:vault_broker, kind: REQUIRES_RUNNING }
  - { unit_id: unit:aios:model_server:llama3_70b, kind: REQUIRES_HEALTHY }
  - { unit_id: unit:aios:evidence_log, kind: REQUIRES_HEALTHY }

sandbox_profile_ref: "prof_ai_agent_worker_floor_user247_001"

verification_intent:
  - type: subject_is_ai_correct
    args: { subject: "agent:user-247:rust-coder" }
  - type: aiosfs_path_in_namespace
    args:
      {
        subject: "agent:user-247:rust-coder",
        expected_root: "/aios/groups/user-247/",
      }
  - type: chrome_zone_no_authorship
    args: { subject: "agent:user-247:rust-coder" }

rollback_pointer:
  aiosfs_pointer_id: "ptr_user247_agents_rust_coder_release"
  expected_current_version_id: "ver_01HY1P3..."
  trigger: ON_HEALTH_FAILURE

resource_budget:
  memory_bytes_max: 4294967296 # 4 GiB
  cpu_quota_cores: 1.5
  disk_bytes_max: 8589934592 # 8 GiB
  file_descriptors_max: 2048
  process_count_max: 16
  queue_depth_max: 256
  gpu:
    requires_compute: false # this worker does not directly drive GPU compute
    vram_bytes_max: 0

restart_policy: ON_FAILURE
restart_budget:
  max_attempts: 3
  reset_window_seconds: 600
  backoff_initial_seconds: 5
  backoff_max_seconds: 120

health_check:
  kind: CUSTOM_PROBE
  probe_interval_seconds: 30
  probe_timeout_seconds: 5
  startup_grace_seconds: 15
  unhealthy_threshold: 2
  healthy_threshold: 1
  args:
    {
      probe_id: "agent.heartbeat",
      expected_subject: "agent:user-247:rust-coder",
    }

startup_deadline_seconds: 60
stop_deadline_seconds: 30
adapter_target:
  agent_binary_pointer: "ptr_agent_vendor_rust_coder_v3_bin"
  config_pointer: "ptr_user247_agents_rust_coder_config"

labels: { layer: "L5", subject_class: "ai", group: "user-247" }
correlation_id: corr_user247_agent_rollout
```

Lifecycle walk:

1. SGR admits. Subject `is_ai = true` triggers AI-class sandbox floor (S3.2 §5.4). INV-013 hard-deny fires if the manifest declares any `system_admin`-class capability.
2. INV-023 verification: `chrome_zone_no_authorship` is in `verification_intent`. The first health check confirms the agent is not authoring CHROME-zone surfaces. INV-023 violation transitions the unit to `UNHEALTHY` immediately with FOREVER evidence per the L0 invariant.
3. Dependencies reach `HEALTHY`. State = `QUEUED → STARTING`. Adapter dispatches under `ActionDispatchKind = ISOLATED_SANDBOX` (S10.1 §3.2: AI-origin actions always upgrade to isolated sandbox).
4. Agent starts inside the AI-class sandbox. State = `STARTING → RUNNING → HEALTHY`. Evidence: `UNIT_STARTED`, `UNIT_HEALTHY`.
5. Adversarial path: queue depth abuse → `DEGRADED` → `UNHEALTHY` → restart per `ON_FAILURE`. If `max_attempts = 3` exhausted within 600 s, → `FAILED`; rollback pointer fires; agent's release pointer in `/aios/groups/user-247/agents/rust-coder/release/` rolls back to `expected_current_version_id`.

## §13 Cross-spec dependencies

| Spec  | Direction  | What this spec contributes                                                                                                                                                                                                                                                                                                                           |
| ----- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S10.1 | producer   | unit-level action kinds (`unit.start`, `unit.stop`, `unit.restart`, `unit.health_probe`, `unit.rollback`) referenced by name; full per-action target schemas live in `04_adapter_model.md`                                                                                                                                                           |
| S3.2  | consumer   | `sandbox_profile_ref` is an id from S3.2; AI-class and recovery-class floors are enforced by S3.2 §5.4 against this manifest's `unit_kind`                                                                                                                                                                                                           |
| S2.4  | consumer   | `verification_intent` entries are S2.4 typed intents; SGR runs them on every health-related transition                                                                                                                                                                                                                                               |
| S1.3  | consumer   | `rollback_pointer` binds to S1.3 `Pointer.current_version_id` via CAS; rollback adapter uses `PromotePointer`                                                                                                                                                                                                                                        |
| S0.1  | consumer   | unit lifecycle actions flow as S0.1 envelopes through S10.1; canonical hashing rule reused                                                                                                                                                                                                                                                           |
| S3.1  | producer   | eight evidence record types queued: `UNIT_REGISTERED`, `UNIT_STARTED`, `UNIT_HEALTHY`, `UNIT_DEGRADED`, `UNIT_FAILED`, `UNIT_STOPPED`, `UNIT_ROLLBACK_TRIGGERED`, `UNIT_DEPENDENCY_CYCLE_DETECTED` (FOREVER); plus narrative-only `MANIFEST_VALIDATION_REJECTED`, `UNIT_REPLAY_REJECTED`, `UNIT_PUBLISHER_TRUST_REVOKED` for next-Wave consolidation |
| S11.1 | consumer   | publisher signature, publisher catalog `pubcat_<hex>`, `PublisherTrustLevel` admission rules; manifest forgery emits S11.1 `MANIFEST_FORGED` / `TRUST_CHAIN_BROKEN`                                                                                                                                                                                  |
| S14.1 | consumer   | resource budget exhaustion classified as `RESOURCE_EXHAUSTION`; behaviors per S14.1 §4.1 rows 24–26                                                                                                                                                                                                                                                  |
| L0    | consumer   | INV-014 (no proof, no completion) — manifests without verification_intent are rejected; INV-017 (sandbox floor constitutional) — `sandbox_profile_ref` cannot loosen the floor; INV-002, INV-013, INV-023 (AI-class units) — agent-worker manifests subject to constitutional checks                                                                 |
| L5    | constraint | `AGENT_WORKER` and `MODEL_SERVER` are the canonical L5 unit kinds; S13.1 cognitive core model treats them as the SGR-side surface                                                                                                                                                                                                                    |
| L6    | constraint | `APP_SESSION` is the L6 app runtime model's per-session surface (S12.1)                                                                                                                                                                                                                                                                              |
| L1    | constraint | `RECOVERY_TASK` and `MOUNT` are the L1 surface; `RECOVERY_TASK` is gated by `is_recovery_mode = true` per INV-012                                                                                                                                                                                                                                    |

## §14 Performance contract

| Operation                            | Budget                                        |
| ------------------------------------ | --------------------------------------------- |
| Manifest validation                  | p95 < 50 ms (signature verify, schema, cycle) |
| Cycle detection (DFS)                | p95 < 20 ms for graphs of ≤ 4096 units        |
| Admission to `QUEUED`                | p95 < 100 ms                                  |
| Probe dispatch (`HTTP_OK`)           | p95 < 200 ms (network probe + verification)   |
| Probe dispatch (`TCP_PORT`)          | p95 < 100 ms                                  |
| Probe dispatch (`COMMAND_EXIT_ZERO`) | p95 < 500 ms (subprocess spawn + exit)        |
| State transition logging             | p95 < 5 ms (one append to S3.1)               |
| Rollback CAS attempt                 | p95 < 100 ms (one S1.3 `PromotePointer`)      |

These budgets apply on the reference hardware described in S2.2 §11 (8-core, 16 GB RAM, NVMe). Probe budgets exclude adapter execution time inside the probe.

## §15 Telemetry contract

| Metric                                     | Type    | Labels (closed)                                           |
| ------------------------------------------ | ------- | --------------------------------------------------------- |
| `sgr_unit_state_total`                     | counter | `unit_kind` (10 values), `to_state` (11 values)           |
| `sgr_unit_admission_total`                 | counter | `outcome` (admitted/rejected/cycle_detected)              |
| `sgr_unit_health_check_duration_seconds`   | hist    | `health_check_kind` (5 values), `outcome`                 |
| `sgr_unit_restart_total`                   | counter | `unit_kind`, `restart_policy`                             |
| `sgr_unit_rollback_total`                  | counter | `rollback_trigger`, `outcome`                             |
| `sgr_unit_resource_budget_exhausted_total` | counter | `bound` (closed enum: memory/cpu/disk/fd/proc/queue/vram) |
| `sgr_active_units`                         | gauge   | `unit_kind`                                               |

Cardinality budget: ≤ 200 active label tuples per metric. Subject is **never** a metric label (per S0.1 §6.6 / S1.2 §9 telemetry discipline).

## §16 Acceptance criteria

- [ ] `UnitKind` is a closed enum with exactly ten values (§3.1).
- [ ] `UnitState` is a closed enum with exactly eleven values (§3.2).
- [ ] `RestartPolicy` is a closed enum with exactly five values (§3.3).
- [ ] `HealthCheckKind` is a closed enum with exactly five values (§3.4).
- [ ] `UnitManifest` declares all of: `unit_id`, `unit_kind`, `dependencies`, `sandbox_profile_ref`, `verification_intent`, `rollback_pointer`, `resource_budget`, `restart_policy`, `health_check`, `publisher_signature`, `canonical_hash` (§4).
- [ ] `unit_id` follows the `unit:<vendor>:<name>[:<variant>]` format and is admitted only if the vendor is in the AIOS-root-signed publisher catalog (§4.1).
- [ ] Canonical hashing follows S0.1 §8 with `hex_lower(BLAKE3(...))[:32]`; signature is over the full 256-bit hash (§4.2).
- [ ] Dependency DAG admission rejects all cycles via DFS (§5.1); emits `UNIT_DEPENDENCY_CYCLE_DETECTED` (FOREVER).
- [ ] Layer-downward dependency (INV-007) is enforced (§5.2).
- [ ] `sandbox_profile_ref` is bound to S3.2 and cannot loosen the constitutional floor (INV-017) (§6).
- [ ] `verification_intent` is required for every kind except `OBSERVER` (§7).
- [ ] `rollback_pointer` binds to S1.3 `Pointer.current_version_id` via CAS; rollback failure transitions the unit to `FAILED` (§8).
- [ ] `resource_budget` is required and enforced; budget exhaustion classified as S14.1 `RESOURCE_EXHAUSTION` (§9).
- [ ] Eight evidence record types queued for S3.1 consolidation (§10).
- [ ] Manifest forgery patterns (§11.2) are rejected with `MANIFEST_FORGED`/`TRUST_CHAIN_BROKEN`/`MANIFEST_VALIDATION_REJECTED`.
- [ ] Three worked examples (§12) produce the documented lifecycle walks.
- [ ] Performance budgets in §14 apply on the reference hardware.
- [ ] Telemetry conforms to §15 cardinality bounds; subject is never a label.

## §17 Open deferrals

- **A/B promotion semantics** — `02_state_transitions.md` will define how a new manifest version supersedes a running unit (rolling, blue/green, recreate).
- **Per-adapter target schemas** — `04_adapter_model.md` will define the per-`unit_kind` and per-action target schemas referenced by `adapter_target`.
- **Multi-host federation** — Rev.2 assumes one authoritative SGR per host. Cross-host unit graphs are deferred.
- **Hot manifest reload** — admitting a new version of an existing `unit_id` requires explicit retirement of the prior version in Rev.2; in-place hot reload is deferred to a follow-on sub-spec.
- **Custom probe schema registry** — `CUSTOM_PROBE` referenced parameter schemas live in the adapter manifest; a centralized cross-publisher probe registry is deferred.
- **Quotas across unit kinds** — per-host quotas (e.g. "no more than 32 `AGENT_WORKER` units active concurrently") are deferred; current Rev.2 enforces only per-unit `resource_budget`.

## See also

- [L3 Overview](00_overview.md)
- [S10.1 Capability Runtime gRPC](03_capability_runtime_grpc.md)
- [S3.2 Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [S2.4 Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [S3.1 Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S0.1 Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S1.3 Object Model](../L2_AIOS_FS/01_object_model.md)
- [S11.1 Repository Model + Trust Roots](../L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [S14.1 Failure Handling and Degradation](../L9_Observability_Admin_Operations/03_failure_handling.md)
- [L0 Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md)
- [Rev.1 §10 — AIOS-SGR](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

Status: REAL
Evidence: E1
