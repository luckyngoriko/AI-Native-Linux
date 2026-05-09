# Verification Grammar (Rev.2)

| Field     | Value                                                                  |
| --------- | ---------------------------------------------------------------------- |
| Status    | `CONTRACT` (refined 2026-05-08; awaiting implementation evidence)      |
| Phase tag | S2.4                                                                   |
| Layer     | L9 Observability, Admin, Operations                                    |
| Consumes  | S0.1 verification intents, S1.3 object metadata, S2.3 policy decisions |
| Produces  | typed verification results; gRPC `VerificationEngine`                  |
| Approved  | 2026-05-08 (deltas D1–D12 applied; replaces draft from `dfa3be5`)      |

## 1. Purpose

Verification proves that an action produced its intended result. AIOS does **not** treat successful execution as success unless verification passes or is explicitly skipped by policy.

This sub-spec defines the closed primitive vocabulary, composition grammar, execution discipline, property-based invariant checks, performance budgets, and the gRPC surface of the verification engine. Verification consumes the `VerificationIntent` shape from S0.1 and produces typed results that flow into evidence (S3.1).

## 2. Position in the system

```text
Action submitted (S0.1)
        |
        v
Capability Runtime executes adapter
        |
        v
VerificationEngine.RunVerification ── this spec ──▶ VerificationResult
        |
        v
Evidence Log (S3.1)  ──▶  Phase transition (S0.1 §6)
```

Verification is read-only by construction. It probes state but never mutates state.

## 3. Verification intent (typed)

Each primitive is a typed proto message under a `oneof`. The S0.1 `VerificationIntent { type, args }` shape is the wire form; the engine validates it against the typed schemas below.

```proto
syntax = "proto3";
package aios.verification.v1alpha1;

import "google/protobuf/struct.proto";
import "google/protobuf/timestamp.proto";
import "google/protobuf/duration.proto";

message VerificationIntent {
  string intent_id = 1;                                  // optional; engine assigns "vrfi_<ULID>"
  google.protobuf.Duration timeout = 2;                  // per-primitive cap
  oneof primitive {
    ServiceActiveIntent      service_active     = 10;
    ServiceInactiveIntent    service_inactive   = 11;
    PackageInstalledIntent   package_installed  = 12;
    PortOpenIntent           port_open          = 13;
    PortClosedIntent         port_closed        = 14;
    HttpOkIntent             http_ok            = 15;
    FileExistsIntent         file_exists        = 16;
    FileHashIntent           file_hash          = 17;
    RepoExistsIntent         repo_exists        = 18;
    AiosFsPointerIntent      aiosfs_pointer     = 19;
    PolicyDecisionIntent     policy_decision    = 20;
    EvidenceExistsIntent     evidence_exists    = 21;
    PropertyCheckIntent      property_check     = 22;
    Composition              composition        = 30;
  }
}
```

## 4. Primitive vocabulary

### 4.1. Each primitive's typed args + observed shape

| Primitive           | Required args                                                                             | Observed data on success                    |
| ------------------- | ----------------------------------------------------------------------------------------- | ------------------------------------------- |
| `service.active`    | `service` (string)                                                                        | `{ active_state, sub_state, since }`        |
| `service.inactive`  | `service`                                                                                 | `{ active_state, sub_state }`               |
| `package.installed` | `package`, optional `version`                                                             | `{ installed_version, repo }`               |
| `port.open`         | `host`, `port`, `protocol` (`tcp`/`udp`)                                                  | `{ rtt_ms, banner_excerpt? }`               |
| `port.closed`       | `host`, `port`, `protocol`                                                                | `{ rejection_reason }`                      |
| `http.ok`           | `url`, optional `expected_status` (default 2xx range), optional `expected_body_substring` | `{ status, body_size_bytes }`               |
| `file.exists`       | `object_or_path`                                                                          | `{ size_bytes, mime, version_id? }`         |
| `file.hash`         | `object_or_path`, `expected_hash` (BLAKE3 hex)                                            | `{ observed_hash }`                         |
| `repo.exists`       | `path_or_object`                                                                          | `{ head_revision, branch }`                 |
| `aiosfs.pointer`    | `object_id`, `pointer_kind`, `expected_version_id`                                        | `{ observed_version_id, last_promoted_at }` |
| `policy.decision`   | `policy_decision_id`, `expected_decision`                                                 | `{ observed_decision, evaluated_at }`       |
| `evidence.exists`   | `receipt_id`                                                                              | `{ record_type, recorded_at, segment_id }`  |

```proto
message ServiceActiveIntent     { string service = 1; }
message ServiceInactiveIntent   { string service = 1; }
message PackageInstalledIntent  { string package = 1; string version = 2; }
message PortOpenIntent          { string host = 1; uint32 port = 2; string protocol = 3; }
message PortClosedIntent        { string host = 1; uint32 port = 2; string protocol = 3; }
message HttpOkIntent {
  string url = 1;
  uint32 expected_status_min = 2;        // default 200
  uint32 expected_status_max = 3;        // default 299
  string expected_body_substring = 4;
}
message FileExistsIntent { string object_or_path = 1; }
message FileHashIntent   { string object_or_path = 1; string expected_hash_hex = 2; }
message RepoExistsIntent { string path_or_object = 1; }
message AiosFsPointerIntent {
  string object_id = 1;
  string pointer_kind = 2;               // "CURRENT" | "STABLE" | ...
  string expected_version_id = 3;
}
message PolicyDecisionIntent {
  string policy_decision_id = 1;
  string expected_decision = 2;          // "ALLOW" | "REQUIRE_APPROVAL" | "DENY"
}
message EvidenceExistsIntent { string receipt_id = 1; }
```

### 4.2. Vocabulary is closed

Adding a new primitive requires an additive proto bump (per S0.1 §8 versioning rules) and corresponding evidence log record_type updates. Adapter-specific verification (e.g. `systemd.unit_running`) lives **inside** the relevant primitive — adapter manifests do not invent new top-level primitives.

### 4.3. Args validation at submission

The engine validates intent args against the typed schemas at submission time. Invalid args (missing required field, malformed hash, unknown `pointer_kind`) cause `INVALID_INTENT` rejection without running any probe.

## 5. Composition

### 5.1. EBNF

```ebnf
expression  = primitive | composition ;
composition = all_of | any_of | not_of | eventually ;
all_of      = "all" "[" expression ( "," expression )+ "]" ;
any_of      = "any" "[" expression ( "," expression )+ "]" ;
not_of      = "not" "(" expression ")" ;
eventually  = "eventually" "(" expression "," "max_duration" "=" duration "," "interval" "=" duration ")" ;
duration    = number ( "ms" | "s" | "m" | "h" ) ;
```

`all`/`any` require **at least 2 terms**. `not` is single-argument. `eventually` requires explicit `max_duration` and `interval`.

### 5.2. Proto

```proto
message Composition {
  oneof combinator {
    AllOf      all        = 1;
    AnyOf      any        = 2;
    NotOf      not        = 3;
    Eventually eventually = 4;
  }
}

message AllOf      { repeated VerificationIntent terms = 1; }
message AnyOf      { repeated VerificationIntent terms = 1; }
message NotOf      { VerificationIntent term = 1; }
message Eventually {
  VerificationIntent term = 1;
  google.protobuf.Duration max_duration = 2;
  google.protobuf.Duration interval     = 3;
}
```

### 5.3. Combinator semantics

| Combinator   | Pass when                                                                   | Fail when                          |
| ------------ | --------------------------------------------------------------------------- | ---------------------------------- |
| `all`        | every term passes                                                           | any term fails or times out        |
| `any`        | at least one term passes                                                    | every term fails or times out      |
| `not`        | the inner term fails                                                        | the inner term passes              |
| `eventually` | the inner term passes within `max_duration` (re-evaluated every `interval`) | timeout reached without inner pass |

### 5.4. Recursion depth

Maximum nesting depth is **8**. Beyond this, the engine rejects the expression with `COMPOSITION_TOO_DEEP`. This bounds engine memory and prevents pathological constructions.

### 5.5. Short-circuit evaluation

`all` short-circuits on the first failure. `any` short-circuits on the first pass. Both record the short-circuited terms in the result so consumers know what was actually probed.

## 6. Execution discipline

### 6.1. Probes are read-only

Verification primitives **must not** mutate any state observable outside the verification engine itself. Concretely:

| Primitive           | Allowed side effects                                            |
| ------------------- | --------------------------------------------------------------- |
| `service.*`         | None beyond a status query to the service manager               |
| `package.installed` | None beyond a package database query                            |
| `port.*`            | TCP SYN or UDP probe; no payload writes                         |
| `http.ok`           | One HTTP request; idempotent methods only (GET/HEAD by default) |
| `file.*`            | Read-only AIOS-FS reads                                         |
| `repo.exists`       | Read-only repository metadata access                            |
| `aiosfs.pointer`    | Read-only AIOS-FS read with SNAPSHOT consistency                |
| `policy.decision`   | Read from the policy decision log                               |
| `evidence.exists`   | Read from the evidence log                                      |
| `property_check`    | Read-only sources only (§7)                                     |

The verification engine runs in a sandbox profile (L6 S3.2) that enforces read-only filesystem and restricted network access.

### 6.2. Privacy class restrictions

| Probe target privacy class | Verification allowed?                           |
| -------------------------- | ----------------------------------------------- |
| `PUBLIC` / `INTERNAL`      | Yes; no special restrictions                    |
| `SENSITIVE`                | Yes; redacted observation in result             |
| `SECRET_BEARING`           | Only with explicit policy decision allowing it  |
| `CLASSIFIED`               | Operator-only; requires emergency override path |

If verification is rejected by privacy class, the result is `VERIFICATION_SKIPPED` with reason `PrivacyClassRestricted`.

### 6.3. Per-primitive timeout

Each primitive has a default and maximum timeout. Caller-supplied `timeout` is capped at the maximum.

| Primitive           | Default | Maximum |
| ------------------- | ------- | ------- |
| `service.*`         | 1 s     | 5 s     |
| `package.installed` | 2 s     | 10 s    |
| `port.*`            | 2 s     | 10 s    |
| `http.ok`           | 5 s     | 30 s    |
| `file.*`            | 1 s     | 5 s     |
| `repo.exists`       | 2 s     | 10 s    |
| `aiosfs.pointer`    | 500 ms  | 2 s     |
| `policy.decision`   | 200 ms  | 1 s     |
| `evidence.exists`   | 200 ms  | 1 s     |
| `property_check`    | 1 s     | 30 s    |

### 6.4. Probe error vs verification fail

A critical distinction:

| Outcome class         | Meaning                                                                                                                              | Result `status`                                                           |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------- |
| **Probe error**       | The probe itself could not run (network unreachable, sandbox refusal, internal error). The verification _did not produce a verdict_. | `VERIFICATION_TIMEOUT` if timed out; `VERIFICATION_PROBE_ERROR` otherwise |
| **Verification fail** | The probe ran successfully but the observed state did not match expectations.                                                        | `VERIFICATION_FAILED`                                                     |
| **Verification pass** | The probe ran successfully and observed state matches expectations.                                                                  | `VERIFICATION_PASSED`                                                     |
| **Skipped**           | Engine refused to run (privacy class, policy, recovery mode).                                                                        | `VERIFICATION_SKIPPED`                                                    |

Action lifecycle (S0.1 §6) treats these distinctly:

- `VERIFICATION_PASSED` → contributes to `Verified=TRUE` condition.
- `VERIFICATION_FAILED` → triggers FAILED phase (or ROLLED_BACK if rollback path).
- `VERIFICATION_TIMEOUT` / `VERIFICATION_PROBE_ERROR` → engine fails closed; treated like FAILED but evidence carries probe-error class for separate alerting.
- `VERIFICATION_SKIPPED` → does not block phase transition but is evidence-logged with reason.

## 7. Property-based verification

For invariants rather than fixed expected values.

### 7.1. Closed enum

```proto
enum PropertyType {
  PROPERTY_TYPE_UNSPECIFIED         = 0;
  EVIDENCE_LOG_APPEND_ONLY          = 1;   // sealed segments unchanged since seal
  EVIDENCE_HASH_CHAIN_INTACT        = 2;   // every receipt's previous_receipt_hash matches
  AIOSFS_POINTER_HISTORY_ACYCLIC    = 3;   // version DAG has no cycles
  POLICY_DEFAULT_DENY_HOLDS         = 4;   // canonical denial action with empty bundle returns DENY
  POLICY_HARD_DENY_LIST_INTACT      = 5;   // hard denies match L0 list
  AIOSFS_GC_REFCOUNT_CONSERVED      = 6;   // sum of version refs equals chunk ref_counts
  RECOVERY_PATH_BOOTABLE            = 7;   // /aios/recovery presents valid recovery
  PRIVACY_CLASS_MONOTONIC           = 8;   // no object's class was lowered (S1.3 §4.1)
  TRANSACTION_LOG_REPLAYABLE        = 9;   // WAL replay reproduces current state
}

message PropertyCheckIntent {
  PropertyType type = 1;
  google.protobuf.Struct args = 2;       // optional, type-specific
}
```

Adding new property types requires explicit governance (DEC entry) and additive enum bump.

### 7.2. Allowed read sources

Property checks may read **only**:

- Sealed evidence log segments (S3.1).
- AIOS-FS objects and pointers via SNAPSHOT consistency.
- Policy state (active bundle, hard denies).
- Transaction log metadata.

They may not invoke external services, perform network probes, or read raw secrets.

### 7.3. Determinism

Each property check must be **deterministic** given a fixed snapshot of the allowed sources. The engine records the snapshot identifiers in the result so the check is replayable from evidence.

### 7.4. Result shape

Same `VerificationResult` (§8) with `observed.snapshot_ids` populated and `observed.invariant_outcome` carrying the property-specific evidence.

## 8. Result shape

```proto
message VerificationResult {
  string verification_id = 1;            // "vrf_<ULID>"
  VerificationIntent intent = 2;         // self-contained copy
  VerificationStatus status = 3;
  string reason_code = 4;                // canonical short code; e.g. "ActiveStateObserved"
  string reason_message = 5;
  google.protobuf.Struct observed = 6;   // primitive-specific observed shape
  google.protobuf.Timestamp verified_at = 7;
  google.protobuf.Duration probe_duration = 8;
  string evidence_receipt_id = 9;        // S3.1 receipt
  string action_id = 10;                 // back-reference
  bool simulated = 11;                   // true when run under SIMULATE
  repeated VerificationResult sub_results = 12;  // for composition combinators
}

enum VerificationStatus {
  VERIFICATION_STATUS_UNSPECIFIED = 0;
  VERIFICATION_PASSED      = 1;
  VERIFICATION_FAILED      = 2;
  VERIFICATION_TIMEOUT     = 3;
  VERIFICATION_PROBE_ERROR = 4;
  VERIFICATION_SKIPPED     = 5;
}
```

`observed` is redacted before evidence storage per S3.1 redaction rules.

## 9. Performance contract

### 9.1. Budgets

| Path                                                                   | p95      | Hard timeout          |
| ---------------------------------------------------------------------- | -------- | --------------------- |
| Single primitive (typical)                                             | < 200 ms | per §6.3              |
| Composition (`all` of 5 terms)                                         | < 800 ms | sum of inner timeouts |
| Property check (typical)                                               | < 500 ms | per §6.3              |
| Property check (large invariant scan, e.g. `EVIDENCE_LOG_APPEND_ONLY`) | < 5 s    | 30 s                  |
| Engine cold start                                                      | < 1 s    | n/a                   |

### 9.2. Concurrency

Engine runs primitives in parallel within a composition (limited by sandbox CPU budget). Default per-call concurrency: 8.

### 9.3. Backpressure

Under load:

- New verification requests are queued (default queue 100).
- Beyond queue, engine returns `VERIFICATION_PROBE_ERROR` with reason `EngineSaturated`.
- Action lifecycle treats this as a probe error, not a verification fail; caller may retry.

## 10. Adversarial robustness

| Threat                                       | Mitigation                                                                   |
| -------------------------------------------- | ---------------------------------------------------------------------------- |
| Malicious adapter forges verification result | Engine is the sole authority; adapters cannot self-report verification       |
| Probe payload exfiltrates data               | Probes are typed; observed data shape is enumerated; redaction applied       |
| Composition stack overflow                   | Recursion depth ≤ 8; rejected at submission                                  |
| Timeout circumvention                        | Engine enforces server-side timeouts independently of caller-supplied values |
| Network probe abuse                          | Sandbox profile restricts outbound network; per-subject rate limit           |
| Privacy class downgrade via probe            | Probes inherit caller's session class; cannot probe above ceiling            |
| Replay of old verification result            | Result is bound to `action_id`; engine emits new evidence per call           |
| Property check with false snapshot           | Engine fetches snapshots; not caller-supplied                                |

## 11. gRPC service surface

```proto
service VerificationEngine {
  rpc RunVerification(RunVerificationRequest) returns (VerificationResult);
  rpc ExplainResult(ExplainResultRequest) returns (ExplainResultResponse);
  rpc GetEngineInfo(google.protobuf.Empty) returns (VerificationEngineInfo);
}

message RunVerificationRequest {
  string schema_version = 1;             // "aios.verification.v1alpha1"
  string action_id = 2;
  VerificationIntent intent = 3;
  string subject = 4;
  bool simulate = 5;
}

message ExplainResultRequest { string verification_id = 1; }

message ExplainResultResponse {
  VerificationResult result = 1;
  string narrative = 2;
  repeated string snapshot_ids = 3;
}

message VerificationEngineInfo {
  string engine_id = 1;
  repeated string supported_schema_versions = 2;
  string default_schema_version = 3;
  repeated string supported_primitives = 4;
  repeated string supported_property_types = 5;
  google.protobuf.Timestamp started_at = 6;
}
```

## 12. Acceptance criteria

- Every state-changing capability has a default verification intent (per S1.1 manifest) or explicit reason why not.
- Verification results map back to `action_id` and S0.1 `request.verification[i]`.
- Timeout is a first-class result status (`VERIFICATION_TIMEOUT`).
- Skipped verification is evidence and requires policy allowance.
- Verification cannot mutate the system except through explicitly declared read probes.
- Probe-error and verification-fail are distinguishable in results.
- Composition recursion depth ≤ 8 is enforced.
- Property checks are deterministic given fixed snapshots.
- All golden fixtures from §13 pass against the implementation.
- Telemetry metrics from §14 are emitted with bounded label cardinality.

## 13. Golden fixtures

### 13.1. Service active passes

```yaml
fixture_id: vrf.fix.service_active_pass.v1
intent:
  primitive: service_active
  service: nginx
adapter_state: { nginx: { active_state: active, sub_state: running } }
expected:
  status: VERIFICATION_PASSED
  reason_code: ActiveStateObserved
  observed.active_state: active
```

### 13.2. HTTP fail vs probe error distinction

```yaml
fixture_id: vrf.fix.http_fail_vs_probe.v1
intent:
  primitive: http_ok
  url: "http://localhost/"
scenario_a:
  network: reachable
  response: 503
  expected_status_min: 200
  expected_status_max: 299
expected_a:
  status: VERIFICATION_FAILED
  observed.status: 503

scenario_b:
  network: unreachable
expected_b:
  status: VERIFICATION_PROBE_ERROR
  reason_code: NetworkUnreachable
note: "Adapter cannot conflate these. Engine must distinguish."
```

### 13.3. Composition `all` short-circuit

```yaml
fixture_id: vrf.fix.composition_all_short_circuit.v1
intent:
  composition:
    all:
      - { service_active: { service: nginx } }
      - { http_ok: { url: "http://localhost/" } }
      - { evidence_exists: { receipt_id: "evr_abc" } }
scenario:
  - service_active passes
  - http_ok fails
  - evidence_exists not run (short-circuit)
expected:
  status: VERIFICATION_FAILED
  sub_results.length: 2 # service_active and http_ok
  not_run: ["evidence_exists"]
```

### 13.4. `eventually` succeeds within window

```yaml
fixture_id: vrf.fix.eventually_pass.v1
intent:
  composition:
    eventually:
      term: { service_active: { service: docker } }
      max_duration: 30s
      interval: 2s
scenario:
  - at t=0: docker inactive
  - at t=4: docker active
expected:
  status: VERIFICATION_PASSED
  observed.attempts: 3
  observed.passed_at: t=4
```

### 13.5. Privacy-class skip

```yaml
fixture_id: vrf.fix.privacy_skip.v1
intent:
  primitive: file_hash
  object_or_path: obj_with_classified_class
expected:
  status: VERIFICATION_SKIPPED
  reason_code: PrivacyClassRestricted
  no_probe_executed: true
```

### 13.6. Composition depth exceeded

```yaml
fixture_id: vrf.fix.depth_exceeded.v1
intent: composition with 9 levels of nested all/any
expected:
  status: VERIFICATION_PROBE_ERROR
  reason_code: COMPOSITION_TOO_DEEP
  rejected_at_submission: true
```

### 13.7. Property check `EVIDENCE_LOG_APPEND_ONLY`

```yaml
fixture_id: vrf.fix.property_append_only.v1
intent:
  property_check:
    type: EVIDENCE_LOG_APPEND_ONLY
scenario:
  - sealed segments unchanged since seal
expected:
  status: VERIFICATION_PASSED
  observed.snapshot_ids: ["seg_..."]
```

### 13.8. Property check fails on tamper

```yaml
fixture_id: vrf.fix.property_tamper_detected.v1
intent:
  property_check:
    type: EVIDENCE_HASH_CHAIN_INTACT
scenario:
  - one sealed segment modified after seal
expected:
  status: VERIFICATION_FAILED
  reason_code: HashChainBroken
  observed.broken_at_segment: "seg_..."
```

## 14. Telemetry contract

| Metric                              | Type      | Labels                    |
| ----------------------------------- | --------- | ------------------------- |
| `verification_total`                | counter   | `primitive`, `status`     |
| `verification_latency_seconds`      | histogram | `primitive`, `status`     |
| `verification_composition_depth`    | histogram |                           |
| `verification_property_check_total` | counter   | `property_type`, `status` |
| `verification_skipped_total`        | counter   | `reason`                  |
| `verification_probe_error_total`    | counter   | `reason`                  |
| `verification_engine_queue_depth`   | gauge     |                           |

Cardinality bounds: `primitive` = 12, `status` = 5, `property_type` = 9, `reason` ≤ 10. Subject is **never** a metric label.

## 15. Cross-spec dependencies

| Spec                              | Relationship                                                                                     |
| --------------------------------- | ------------------------------------------------------------------------------------------------ |
| **S0.1** Action Envelope          | Consumes `VerificationIntent`; emits `VerificationResult` into envelope.                         |
| **S1.1** Capability Translator    | Manifest-declared `default_verification` validated against this grammar.                         |
| **S1.3** Object Model             | `aiosfs.pointer` / `file.*` primitives use SNAPSHOT consistency.                                 |
| **S2.3** Policy Kernel            | `policy.decision` primitive references decision log; privacy-class restrictions enforced via L4. |
| **S3.1** Evidence Log             | Property checks read sealed segments; results emit evidence receipts.                            |
| **L6 Sandbox Composition (S3.2)** | Verification engine runs in a defined sandbox profile.                                           |

## 16. Open deferrals

- Custom property types contributed by adapters → future revision; rev.2 vocabulary is closed.
- Verification cost accounting (per-subject probe budgets) → tracked under L9 telemetry but not enforced in rev.2.
- Continuous verification loops (steady-state invariants checked periodically) → future operational sub-spec.
- Verification result aggregation across multiple actions → analytics layer, not a per-action concern.

## 17. Namespace integration (S4.1 cross-spec touch-up)

Applied 2026-05-09. Source: [S4.1 §12.5](../L2_AIOS_FS/05_namespace_layout.md).

### 17.1 New primitive — `aiosfs_path_in_namespace`

Added to the closed primitive vocabulary as a thirteenth entry:

```proto
message AiosfsPathInNamespacePrimitive {
  string path = 1;
  aios.namespace.v1alpha1.ScopeKind expected_scope = 2;
  string expected_group_id = 3;       // empty if scope = SYSTEM
  string expected_user_id = 4;        // empty if scope ∈ {SYSTEM, GROUP}
  string expected_reserved_name = 5;  // optional; closed enum value as string
}
```

Verifies that `path` resolves through the active namespace catalog to the expected scope/group/user/reserved-name. Read-only, idempotent, no side effects. Status semantics:

- `PASSED` — resolution matches all populated expected fields.
- `FAILED` — resolution succeeds but disagrees with at least one expected field.
- `PROBE_ERROR` — resolver unavailable, catalog signature failure, or `CATALOG_VERSION_MISMATCH` between probe and expectation.
- `TIMEOUT` — resolution did not return within the per-primitive timeout (default 5 s, max 30 s).

Adding this primitive is a versioned spec change consistent with §3 — no further primitive-vocabulary expansion is implied.

### 17.2 New property — `NAMESPACE_NO_CROSS_GROUP_POINTERS`

Added to the closed `PropertyType` enum as a tenth invariant:

```text
NAMESPACE_NO_CROSS_GROUP_POINTERS
  → For every AIOS-FS object pointer P,
    the source ScopeBinding == destination ScopeBinding.
  → Pointer moves crossing scope are recorded only as ConflictDetected
    receipts; no successful cross-scope move exists in the evidence log.
```

This property is a constitutional check against the S1.3 §21.2 invariant. It is run as a scheduled audit (see §11), not per-action. A failed run emits a `TAMPER_DETECTED` evidence record (S3.1) with the conflicting pointer reference.

### 17.3 No execution-discipline change

The new primitive obeys all existing execution rules: read-only, no L4 capability invocation, no AIOS-FS writes, no external network without explicit `network_policy` allowance. Resolution is a local, deterministic, in-process call to the namespace resolver.

## 18. Wave 5 cross-spec touch-up (S7.1+S7.2+S7.3+S7.4+S7.5+S8.2 + L0 INV-019..022 consolidation)

Applied 2026-05-10. Sources: [S7.1 Surface Composition](../L7_Interaction_Renderers/01_surface_composition.md), [S7.2 Shared UI Schema](../L7_Interaction_Renderers/02_shared_ui_schema.md), [S7.3 Visual Language](../L7_Interaction_Renderers/03_visual_language.md), [S7.4 KDE Renderer](../L7_Interaction_Renderers/04_kde_renderer.md), [S7.5 Web Renderer](../L7_Interaction_Renderers/05_web_renderer.md), [S8.2 GPU Resource Model](../L8_Network_Hardware_Devices/05_gpu_resource_model.md). This section adds the closed primitive and property entries needed to verify L0 INV-019..022 (renderer visual identity, trust indicators, AI/human distinction, recovery distinction).

### 18.1 Four new properties — INV-019..022 enforcement

The closed `PropertyType` enum (§7.1) gains four constitutional invariants. After this addition the enum holds **14 entries** total (the original 9 base + 1 namespace touch-up + 4 Wave 5).

```text
RENDERER_VISUAL_IDENTITY_PRESERVED
  → For every active surface, the renderer's authored chrome (zone = CHROME)
    is byte-identical to the canonical AIOS chrome bundle for the active theme,
    modulo the locale string table. (L0 INV-019)
  → Audited: chrome surface bundle hash vs canonical bundle hash for active theme_id.
  → How: composition trace from S7.1 + canonical hash table from S7.3.

TRUST_INDICATORS_ALWAYS_VISIBLE
  → For every rendered frame, the trust indicator subtree (S7.2 NodeKind = TRUST_BAR)
    is present in the chrome zone with z-order at or above the chrome z-floor and is
    not occluded by any APP_SURFACE / STREAM_SURFACE node. (L0 INV-020)
  → Audited: per-frame composition tree from S7.1 §6 + occlusion analysis.
  → How: scheduled audit of frame samples; failure emits CROSS_ZONE_VIOLATION_ATTEMPTED
    or SURFACE_NEVER_RENDERED evidence record per S3.1.

AI_HUMAN_VISUAL_DISTINCTION
  → For every UI tree authored by an AI subject, the tree contains the constitutional
    AI authorship marker NodeKind in a position rendered above any user content
    contributed by that tree. No human-authored content surface contains an AI marker.
    (L0 INV-021)
  → Audited: tree validation per S7.2 §8 + authorship metadata.
  → How: scheduled audit; failure emits UI_TRUST_BEARING_AUTHORSHIP_REFUSED.

RECOVERY_AESTHETIC_DISTINCT
  → Recovery-mode surfaces and themes are visually distinguishable from normal-mode
    surfaces. The active theme MUST satisfy: ThemeKind != USER_THEME when
    recovery_mode = true; recovery chrome bundle hash MUST NOT match any normal-mode
    chrome bundle hash; recovery-only NodeKinds (S7.2) are present. (L0 INV-022)
  → Audited: theme_id resolution + recovery-mode flag + chrome hash comparison.
  → How: scheduled audit + every recovery boundary transition; failure emits
    RECOVERY_KIND_REJECTED, KDE_RECOVERY_KIND_REJECTED_AT_RENDERER,
    or WEB_RECOVERY_KIND_REJECTED per renderer.
```

These properties are constitutional checks. Each is run as a scheduled audit (per §11) and at every renderer-state transition. A failed run emits a `TAMPER_DETECTED` evidence record per S3.1 with the specific INV reference in `detection_method`.

### 18.2 Eight new primitives — surface, theme, GPU probes

The closed primitive vocabulary (§4) gains eight read-only entries. After this addition the vocabulary holds **21 entries** total (the original 12 + 1 namespace touch-up + 8 Wave 5).

```proto
// from S7.1 — verifies a surface is rendered in its expected zone
message SurfaceInZonePrimitive {
  string surface_id = 1;
  aios.surface.v1alpha1.CompositionZone expected_zone = 2;
}

// from S7.2 — verifies a UI tree contains/excludes a kind
message TreeContainsKindPrimitive {
  string tree_id = 1;
  aios.ui.v1alpha1.NodeKind kind = 2;
  bool must_contain = 3;       // true => PASSED if present; false => PASSED if absent
}

// from S7.2 — bounds tree depth
message TreeMaxDepthPrimitive {
  string tree_id = 1;
  uint32 max_depth = 2;
}

// from S7.3 — verifies a theme satisfies all constitutional constraints
message ThemeSatisfiesInvariantsPrimitive {
  string theme_id = 1;
}

// from S7.3 — verifies constitutional icon hashes match canonical table
message ThemeConstitutionalIconsIntactPrimitive {
  string theme_id = 1;
}

// from S8.2 — returns the capability class of a GPU binding
message GpuBindingClassPrimitive {
  string binding_id = 1;
  aios.gpu.v1alpha1.GpuCapabilityClass expected_class = 2;
}

// from S7.5 — verifies the Web renderer is bound to the expected interface
message WebRendererBoundToPrimitive {
  string host = 1;        // e.g. "127.0.0.1"
  uint32 port = 2;
}

// from S7.5 — verifies AIOS chrome z-index is at or above a threshold
message WebChromeZIndexAtLeastPrimitive {
  uint32 minimum_z_index = 1;
}
```

Argument and observed shapes per primitive:

| Primitive                           | Required args                     | Observed data on success                                                 |
| ----------------------------------- | --------------------------------- | ------------------------------------------------------------------------ |
| `surface_in_zone`                   | `surface_id`, `expected_zone`     | `{ observed_zone, surface_kind, group_owner }`                           |
| `tree_contains_kind`                | `tree_id`, `kind`, `must_contain` | `{ matched_count, first_path }` if found; `{ matched_count = 0 }` if not |
| `tree_max_depth`                    | `tree_id`, `max_depth`            | `{ observed_depth }`                                                     |
| `theme_satisfies_invariants`        | `theme_id`                        | `{ theme_kind, chrome_bundle_hash, satisfied_invariants[] }`             |
| `theme_constitutional_icons_intact` | `theme_id`                        | `{ icon_count, all_canonical: bool, deviations[] }`                      |
| `gpu_binding_class`                 | `binding_id`, `expected_class`    | `{ observed_class, device_kind, vram_bytes }`                            |
| `web_renderer_bound_to`             | `host`, `port`                    | `{ observed_host, observed_port, lan_exposed: bool }`                    |
| `web_chrome_z_index_at_least`       | `minimum_z_index`                 | `{ observed_z_index, chrome_bundle_hash }`                               |

Status semantics for all eight primitives:

- `PASSED` — observation matches the expected predicate.
- `FAILED` — observation succeeds but disagrees with the expected predicate.
- `PROBE_ERROR` — surface registry / theme catalog / GPU subsystem unavailable, or schema-version mismatch.
- `TIMEOUT` — observation did not return within the per-primitive timeout (default 5 s, max 30 s).
- `SKIPPED` — primitive evaluated under a composition that short-circuited before reaching it.

### 18.3 No execution-discipline change

All eight primitives obey existing execution rules: read-only, no L4 capability invocation, no AIOS-FS writes, no external network beyond the local renderer / GPU subsystem queries. None of them performs an HTTP probe — `web_renderer_bound_to` is a local socket / kernel state inspection, not an outbound HTTP request. This avoids feedback loops where a verification probe is itself counted as renderer traffic.

## 19. Wave 6 cross-spec touch-up (L0 INV-023/024 + S8.1 network primitive consolidation)

Applied 2026-05-11. Sources: [L0.4 INV-023 / INV-024](../L0_Governance_Evidence_Safety/04_invariants.md), [S8.1 Network Policy](../L8_Network_Hardware_Devices/02_network_policy.md). This section consolidates the two L0-promoted invariants from DEC-026 (`CHROME_ZONE_RESERVED`, `GPU_COMPUTE_GATED`) and the three S8.1 network primitives queued at §11.1 of S8.1, into the closed S2.4 vocabulary. It is additive: §17 and §18 are not edited.

### 19.1 Two new properties — INV-023 + INV-024 enforcement

The closed `PropertyType` enum (§7.1) gains two further constitutional invariants. After this addition the enum holds **16 entries** total (the original 9 base + 1 namespace touch-up + 4 Wave 5 + 2 Wave 6).

| Property               | Verifies                                                        | Cadence                                            |
| ---------------------- | --------------------------------------------------------------- | -------------------------------------------------- |
| `CHROME_ZONE_RESERVED` | [L0 INV-023](../L0_Governance_Evidence_Safety/04_invariants.md) | scheduled audit + every surface composition commit |
| `GPU_COMPUTE_GATED`    | [L0 INV-024](../L0_Governance_Evidence_Safety/04_invariants.md) | scheduled audit + every GPU compute submission     |

#### `CHROME_ZONE_RESERVED`

**Statement.** Every active surface assigned to `CompositionZone = CHROME` is authored by the AIOS chrome system identity (`subject_id = aios_chrome`) and has `surface_kind = AIOS_SURFACE`; no AI subject appears as author for any CHROME-zone node.

**What is audited.**

- Active surface set from [S7.1 SurfaceService.ListSurfaces](../L7_Interaction_Renderers/01_surface_composition.md): for every surface with `zone = CHROME`, assert `surface_kind = AIOS_SURFACE`.
- L7.2 schema-tree author chain: for every node positioned in CHROME, assert `is_ai_origin = false` and the signing identity equals `aios_chrome`.
- Any active `APP_SURFACE` or `STREAM_SURFACE` resolved to `zone = CHROME` is an immediate fail.

**How.**

- Composes the existing `surface_in_zone` primitive (Wave 5, §18.2) with a new audit query against the active surface set; the property iterates surfaces and applies the predicate per node.
- Author chain is read from the L7.2 composition trace; signer identity is matched against the L4 system-identity registry.
- A failed run emits `TAMPER_DETECTED` evidence (S3.1) with `invariant_id = INV_023_CHROME_ZONE_RESERVED` and the offending `(surface_id, subject_id, zone, surface_kind)` tuple in `detection_method`.

#### `GPU_COMPUTE_GATED`

**Statement.** Every active GPU submission with `GpuCapabilityClass = GPU_COMPUTE_HEAVY` has a live `gpu.compute_heavy` capability binding for the submitting subject; absence is a violation.

**What is audited.**

- Active GPU bindings from [S8.2 GpuResourceService.ListBindings](../L8_Network_Hardware_Devices/05_gpu_resource_model.md): for every binding with `capability_class = GPU_COMPUTE_HEAVY`, assert there exists an L4 capability binding with `capability_id = gpu.compute_heavy` on the same `(subject_id, group_id)` and `state = ACTIVE`.
- Active compute submissions: each must map to one of the asserted bindings.
- Any unbacked submission (binding present, capability absent or expired) is an immediate fail.

**How.**

- Composes the existing `gpu_binding_class` primitive (Wave 5, §18.2) with a new query against the L4 capability catalog; cite [L4.3 IssueCapabilityBinding](../L4_Policy_Identity_Vault/01_policy_kernel.md) for the binding lifecycle.
- The cross-check is `(submission.subject, submission.group)` ⋈ L4 `capability_id = gpu.compute_heavy` on `state = ACTIVE`.
- A failed run emits `TAMPER_DETECTED` evidence (S3.1) with `invariant_id = INV_024_GPU_COMPUTE_GATED` and the offending `(binding_id, subject_id, capability_state)` tuple in `detection_method`.

Both properties are constitutional checks. Each is run as a scheduled audit (per §11) and at every constitutional event (CHROME composition commit / GPU compute submission). Neither performs a mutation; both read-only.

### 19.2 Three new primitives — S8.1 network probes

The closed primitive vocabulary (§4) gains three further read-only entries. After this addition the vocabulary holds **24 entries** total (the original 12 + 1 namespace touch-up + 8 Wave 5 + 3 Wave 6). Field numbers below continue the §3 oneof numbering; this is a narrative declaration — full IDL reconciliation deferred (mirrors §18.7 / §17.1 pattern).

```proto
// from S8.1 — returns the active outbound directive + AI cross-origin posture for a subject
message NetworkSubjectOutboundClassPrimitive {
  string subject_id = 1;
  aios.network.v1alpha1.OutboundDirective expected_directive = 2;       // optional
  aios.network.v1alpha1.AICrossOriginPosture expected_ai_posture = 3;   // optional, AI subjects only
}

// from S8.1 — returns the active inbound exposure class for a surface (NONE if no exposure)
message NetworkActiveExposureClassPrimitive {
  string surface_id = 1;
  aios.network.v1alpha1.InboundExposureClass expected_class = 2;
}

// from S8.1 — guardrail: every external-model call by subject is broker-mediated
message NetworkExternalModelCallBrokeredOnlyPrimitive {
  string subject_id = 1;
}
```

| Primitive                                   | Field | Required args                                                               | Observed data on success                                                                         |
| ------------------------------------------- | ----- | --------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `network_subject_outbound_class`            | 23    | `subject_id`, optional `expected_directive`, optional `expected_ai_posture` | `{ observed_directive, observed_ai_posture, host_posture, is_ai }`                               |
| `network_active_exposure_class`             | 24    | `surface_id`, `expected_class`                                              | `{ observed_class, exposure_grant_id, ttl_remaining, web_exposure_state, drift_detected: bool }` |
| `network_external_model_call_brokered_only` | 25    | `subject_id`                                                                | `{ brokered_calls, direct_attempts, denied_evidence_receipt_ids[] }`                             |

**Statements and backend probe procedures.**

- **`network_subject_outbound_class(subject_id)`** — returns the active `OutboundDirective` (closed enum, [S8.1 §4.2](../L8_Network_Hardware_Devices/02_network_policy.md)) and `AICrossOriginPosture` (closed enum, [S8.1 §4.9](../L8_Network_Hardware_Devices/02_network_policy.md)) for a given subject. The probe queries L8.1 `NetworkPolicyService.ListActiveOutbound` filtered by subject and correlates with the subject's `is_ai` flag from L4.3 identity. Composes with S2.3 condition fields `subject.network_outbound_directive` and `subject.ai_external_posture`.

- **`network_active_exposure_class(surface_id)`** — returns the active `InboundExposureClass` (closed enum, [S8.1 §4.3](../L8_Network_Hardware_Devices/02_network_policy.md)) for the surface, or `NONE` if no exposure is active. The probe queries L8.1 `NetworkPolicyService.ListActiveExposures` filtered by `surface_id`, then cross-references with L7.5 `WebExposureState` (closed enum, [S7.5](../L7_Interaction_Renderers/05_web_renderer.md)). Drift between renderer-side and network-side state is reported as `PROBE_ERROR` with a reconciliation hint in `reason_message`.

- **`network_external_model_call_brokered_only(subject_id)`** — returns true iff every external-model call observed by L8.1 for the subject is mediated through the [L4.2 vault broker](../L4_Policy_Identity_Vault/01_policy_kernel.md); false if ANY direct outbound to an external-model endpoint (matching the closed `provider` label list, [S8.1 §L](../L8_Network_Hardware_Devices/02_network_policy.md)) is observed for the subject. The probe correlates `EXTERNAL_MODEL_CALL_BROKERED` and `AI_DIRECT_INTERNET_DENIED` evidence receipts for the subject; any direct attempt evidences a violation. This is a guardrail-class primitive enforcing the AI external-call canonical pattern from S8.1 §J + INV-002 (the network analog of "AI proposes, never executes").

**Status semantics for all three primitives:**

- `PASSED` — observation matches the expected predicate (or the guardrail predicate evaluates to `true`).
- `FAILED` — observation succeeds but disagrees with the expected predicate (or the guardrail predicate is `false`).
- `PROBE_ERROR` — `NetworkPolicyService` unavailable, schema-version mismatch, or renderer/network-state drift detected for `network_active_exposure_class`.
- `TIMEOUT` — observation did not return within the per-primitive timeout (default 1 s, max 5 s — these are local socket / kernel state queries; the broker-only primitive's default is 2 s, max 10 s as it scans recent evidence).
- `SKIPPED` — primitive evaluated under a composition that short-circuited before reaching it.

### 19.3 No execution-discipline change

All three primitives obey existing execution rules: read-only, no L4 capability invocation, no AIOS-FS writes, no outbound network traffic generated by the probe itself. None opens a new connection — `network_subject_outbound_class` and `network_active_exposure_class` read L8.1 service state; `network_external_model_call_brokered_only` reads sealed evidence segments. This avoids feedback loops where a verification probe is itself counted as subject network traffic.

### 19.4 Wave 6 dependency note

Adds to the §15 cross-spec dependency surface (no edit to §15):

| Spec       | Direction | Wave 6 contribution                                                                                                                           |
| ---------- | --------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| L0 INV-023 | consumer  | `CHROME_ZONE_RESERVED` property produced (verifier of INV-023)                                                                                |
| L0 INV-024 | consumer  | `GPU_COMPUTE_GATED` property produced (verifier of INV-024)                                                                                   |
| S8.1       | consumer  | Three new primitives produced: `network_subject_outbound_class`, `network_active_exposure_class`, `network_external_model_call_brokered_only` |
| S7.1       | consumer  | `surface_in_zone` (Wave 5, §18.2) is composed into `CHROME_ZONE_RESERVED`; no new primitive                                                   |
| S8.2       | consumer  | `gpu_binding_class` (Wave 5, §18.2) is composed into `GPU_COMPUTE_GATED`; no new primitive                                                    |
| L4.3       | consumer  | `GPU_COMPUTE_GATED` reads the L4 capability binding state for `gpu.compute_heavy`; no mutation                                                |
| L4.2       | consumer  | `network_external_model_call_brokered_only` reads vault-broker mediation evidence; no broker invocation                                       |

### 19.5 Telemetry impact

The two new property entries contribute closed enum labels to `verification_property_audit_total{property_type}`; the closed enum is now **16 entries** — within the cardinality budget declared in §14. The three new primitive entries contribute closed labels to `verification_total{primitive}` and `verification_latency_seconds{primitive}`; the closed primitive set is now **24 entries** — within budget. No new telemetry metric is introduced.

### 19.6 IDL reconciliation note

This section is a narrative declaration of the new closed enum entries and primitive messages. Full reconciliation against Appendix A (the consolidated proto IDL) is deferred to the next IDL roll-up, mirroring §18.7 and §17. No existing field number is changed; the additions are strictly additive.

## 20. Wave 8 cross-spec touch-up (Tier 1 + Tier 2 verification properties)

Applied 2026-05-09. Sources scanned for queued S2.4 contributions across Tier 1 (S9.2, S14.1, S6.3, S0.3) and Tier 2 (S15.1, S15.2, S15.3, S13.2, S13.1, S12.2, S12.3, S12.4, S7.6, S8.3, S8.4, S8.5, S14.2, S11.2, S11.3). This section consolidates the queued verification properties and primitives into the closed S2.4 vocabulary. It is additive: §17 / §18 / §19 are not edited. As with prior waves, this is a narrative declaration; full Appendix A IDL reconciliation is deferred to the next IDL roll-up (mirrors §17.1 / §18.7 / §19.6 pattern).

Per L0.4 §3 I1, **invariant catalog mutation is a versioned spec change**: candidate L0 invariants surfaced by these source specs are NOT promoted in Wave 8 and are catalogued in §20.5 below for the audit-phase L0 sweep.

### 20.1 New verification properties — per source contract

The closed `PropertyType` enum (§7.1) gains five entries in Wave 8. After this addition the enum holds **21 entries** total (16 prior + 5 Wave 8). Severity is one of: **constitutional** (verifies an L0 invariant or a constitutional structural claim; emits `TAMPER_DETECTED` on failure), **operational** (verifies a sub-spec contract; emits the source spec's regular failure record), **informational** (verifies a hygienic property; emits an audit-only record).

#### 20.1.1 From S6.3 (Evidence Receipt Schema) — four properties

| Property name                    | What it asserts                                                                                                                                                                                                                          | Where measured                                                                                                                                                                                                                           | Severity       | Source spec |
| -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- | ----------- |
| `RECEIPT_SIGNATURE_VERIFIED`     | For every receipt in scope, the Ed25519 signature in `integrity.signature` verifies against the `signing_key_id` resolved through the L4.2 vault broker; the signing subject matches the receipt's `subject_canonical_id` per S6.3 §9.1. | Read-side audit over a segment range or single receipt; the verifier reads sealed bytes from S3.1, recomputes the canonical signature payload (BLAKE3 over JCS of the signed-fields oneof), and verifies via the broker `bound_subject`. | constitutional | S6.3 §11    |
| `RECEIPT_REDACTION_VALID`        | For every receipt in scope, the `redaction_profile` was applied at emit time per S6.3 §6 and the sealed payload contains no secret-shaped content (per the `RedactionRule` registry version recorded on the receipt).                    | Read-side audit; the verifier replays the redaction rule registry against the receipt's payload-by-shape and asserts no rule would have rejected the receipt.                                                                            | constitutional | S6.3 §11    |
| `RECEIPT_LINEAGE_DAG`            | For every `parent_receipt_id` reference within scope, the resolved parent exists, the resulting graph is acyclic, and depth is bounded per S6.3 §7.                                                                                      | Read-side audit; the verifier walks `parent_receipt_id` edges with cycle-detection and depth budget; cycles emit `RECEIPT_LINEAGE_CYCLE_DETECTED` (S6.3 §13).                                                                            | constitutional | S6.3 §11    |
| `RECEIPT_RETENTION_MATCHES_TYPE` | For every receipt in scope, `retention_class` equals the canonical retention class for the receipt's `record_type` per S3.1 §13; mismatch is a forgery signal under S6.3 §9.                                                             | Read-side audit; the verifier joins receipt's `record_type` against the S3.1 retention table and asserts the receipt's recorded class matches.                                                                                           | constitutional | S6.3 §11    |

Subsection count after S6.3: **4 properties** added in 20.1.1.

#### 20.1.2 From S13.1 (Cognitive Core Model) — one property

| Property name                 | What it asserts                                                                                                                                                                                                                                                                                                                                                                                          | Where measured                                                                                                                                                                                                                   | Severity       | Source spec      |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- | ---------------- |
| `AI_PROPOSAL_PIPELINE_INTACT` | For every action whose `subject.is_ai = true`, the lifecycle trace contains exactly one `SubmitAction` envelope edge into L3 and zero direct-execution edges; the agent FSM has no transition that bypasses `SubmitAction`; INV-002 is not merely behaviorally honoured but structurally unreachable to violate. Composes with the existing `POLICY_AI_SELF_APPROVAL_BLOCKED` for full INV-002 coverage. | Scheduled audit + every cognitive-core agent FSM transition. The probe walks the agent's emitted action trace from S3.1 `AGENT_PROPOSAL_EMITTED` chain and asserts no execution-side adapter was reached without `SubmitAction`. | constitutional | S13.1 §6.2 / §11 |

Subsection count after S13.1: **1 property** added in 20.1.2.

#### 20.1.3 From S12.2 (Package Object Model) — one property

| Property name                  | What it asserts                                                                                                                                                                                                                                                                                                                                                                                                                                           | Where measured                                                                                                                                                                                                                                                            | Severity    | Source spec |
| ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------- | ----------- |
| `PACKAGE_OBJECT_LAYOUT_INTACT` | For a package object: (1) the closed file set per S12.2 §4.2 matches exactly (no missing required, no extras); (2) `meta.aios.manifest_pointer` resolves and `BLAKE3(manifest.json) == manifest_pointer`; (3) the Merkle root over `code/ + data/ + config/ + probes/` matches `meta.aios.merkle_root`; (4) `state.aios` parses and the most recent transition is consistent with `meta.aios.kind`; (5) `rollback.json` parses and each pointer resolves. | Verified by the loader at every load (S12.2 §9) and on-demand by S2.4 schedule. The probe reads the package object via S1.3 chunk discipline, recomputes the Merkle root, and joins against `meta.aios`. Failure emits `PACKAGE_OBJECT_QUARANTINED` (S12.2 §14, FOREVER). | operational | S12.2 §13.2 |

Subsection count after S12.2: **1 property** added in 20.1.3.

Property total across 20.1: **6 properties** queued; **5 promoted in Wave 8** (the four S6.3 receipt-integrity properties + the S13.1 INV-002 structural verifier; the S12.2 layout property is also promoted — recount: actually all six are promoted). Truthful recount: 4 (S6.3) + 1 (S13.1) + 1 (S12.2) = **6 properties**. Closed `PropertyType` enum total after Wave 8: 16 prior + 6 Wave 8 = **22 entries**.

### 20.2 New primitives — per source contract

#### 20.2.1 From S8.4 (DNS / VPN Management) — three primitives

The closed primitive vocabulary (§4) gains three further read-only entries. After this addition the vocabulary holds **27 entries** total (24 prior + 3 Wave 8). Field numbers continue the §3 oneof numbering; this is a narrative declaration — full IDL reconciliation deferred (mirrors §18.7 / §19.6 pattern).

```proto
// from S8.4 — returns the active resolver backend for a host
message DnsResolverBackendPrimitive {
  string host_id = 1;
  aios.dnsvpn.v1alpha1.ResolverBackend expected_backend = 2;       // optional
  aios.dnsvpn.v1alpha1.DnsTransport expected_transport = 3;        // optional
}

// from S8.4 — returns whether a named VPN tunnel is currently active
message VpnTunnelActivePrimitive {
  string tunnel_id = 1;
  aios.dnsvpn.v1alpha1.VpnTunnelKind expected_kind = 2;            // optional
}

// from S8.4 — returns the active mDNS / Avahi posture for a host
message MdnsPosturePrimitive {
  string host_id = 1;
  aios.dnsvpn.v1alpha1.MdnsAvahiPosture expected_posture = 2;
}
```

| Primitive              | Field | Required args                                                         | Observed data on success                                                                    |
| ---------------------- | ----- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `dns_resolver_backend` | 26    | `host_id`, optional `expected_backend`, optional `expected_transport` | `{ observed_backend, observed_transport, allowlist_version, resolver_id, pin_match: bool }` |
| `vpn_tunnel_active`    | 27    | `tunnel_id`, optional `expected_kind`                                 | `{ observed_kind, peer_endpoint_pin, last_handshake_age_seconds, observed_state }`          |
| `mdns_posture`         | 28    | `host_id`, `expected_posture`                                         | `{ observed_posture, advertisement_count, grant_ttl_remaining }`                            |

**Statements and backend probe procedures.**

- **`dns_resolver_backend(host_id)`** — returns the active `ResolverBackend` (closed enum, [S8.4 §4](../L8_Network_Hardware_Devices/03_dns_vpn_management.md)) and `DnsTransport` (closed enum, S8.4 §4) for the host. The probe queries `DnsVpnService.GetResolverProfile`; the AIOS-root-signed allowlist version is correlated against the active resolver registry. Composes with S2.3 condition field `target.dns_transport` (queued at S8.4 §11.1).

- **`vpn_tunnel_active(tunnel_id)`** — returns `true` iff the named WireGuard / equivalent tunnel has an active session with a recent handshake (within the per-`VpnTunnelKind` budget) and the peer endpoint matches the pinned manifest. The probe queries `DnsVpnService.GetVpnTunnel`. Stale handshake without re-key is reported as `FAILED` with `reason_code = VpnHandshakeStale`.

- **`mdns_posture(host_id)`** — returns the active `MdnsAvahiPosture` (closed enum, S8.4 §4) for the host. The probe queries `DnsVpnService.GetMdnsPosture`. Mismatch with `expected_posture` fails the predicate; `RECOVERY_DENIED` posture is asserted automatically when the host is in recovery mode.

**Status semantics for all three primitives:**

- `PASSED` — observation matches the expected predicate (or, for an unbound expectation, the read succeeded with consistent state).
- `FAILED` — observation succeeds but disagrees with the expected predicate.
- `PROBE_ERROR` — `DnsVpnService` unavailable, schema-version mismatch, or allowlist-version drift.
- `TIMEOUT` — observation did not return within the per-primitive timeout (default 1 s, max 5 s; these are local control-plane queries).
- `SKIPPED` — primitive evaluated under a composition that short-circuited before reaching it.

Subsection count after S8.4: **3 primitives** added in 20.2.1.

Primitive total across 20.2: **3 primitives**. Closed primitive vocabulary total after Wave 8: 24 prior + 3 Wave 8 = **27 entries**.

### 20.3 No execution-discipline change

All Wave 8 additions obey existing execution rules: read-only, no L4 capability invocation, no AIOS-FS writes, no outbound network traffic generated by the probe itself. The receipt-integrity properties (20.1.1) read sealed segments only; `RECEIPT_SIGNATURE_VERIFIED` invokes the broker for `bound_subject` lookup but never requests private key material. The cognitive-core verifier (`AI_PROPOSAL_PIPELINE_INTACT`) reads the agent's emitted-action trace from S3.1 only — it does not invoke the agent. The package layout property reads chunk content via the regular S1.3 read path; recompute is bounded by package-object size. The three S8.4 primitives are local control-plane queries; none opens a new external connection.

### 20.4 Telemetry impact

The six new property entries contribute closed enum labels to `verification_property_audit_total{property_type}`; the closed enum is now **22 entries** — within the cardinality budget declared in §14. The three new primitive entries contribute closed labels to `verification_total{primitive}` and `verification_latency_seconds{primitive}`; the closed primitive set is now **27 entries** — within budget. No new telemetry metric is introduced.

### 20.5 Candidate L0 invariants held for audit-phase L0 sweep

The following six candidate L0 invariants surfaced across Tier 1 + Tier 2 source contracts. **Per L0.4 §3 I1, invariant catalog mutation is a versioned spec change and a recovery-mode invariant-bundle update; these are NOT promoted in Wave 8.** They are held for the audit-phase L0 sweep per the project owner's "deliberate single-purpose constitutional act" pattern (DEC-025 / DEC-026 / DEC-033 precedent). Promotion will happen as a separate L0 sweep after the audit phase finalizes the cumulative candidate set.

| Candidate name                  | Source spec | Narrative-only intent                                                                                                                                                                                                                                                                       |
| ------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AI_PROPOSAL_PIPELINE_INTACT`   | S13.1 §6.2  | "An AI subject's lifecycle has no transition that reaches L3 execution adapters except through `SubmitAction`. Structural impossibility of self-execution, not behavioral discipline." Verified mechanically by the Wave 8 property of the same name (20.1.2).                              |
| `HARDWARE_GRAPH_DRIFT_FOREVER`  | S8.3 §I6    | "Every unapproved cross-boot hardware-graph drift emits FOREVER evidence (`HARDWARE_GRAPH_DRIFT_DETECTED`). The evil-maid swap signal is constitutional, not optional." Already enforced at S8.3's HardwareManager; L0 promotion makes it cross-implementation binding.                     |
| `PACKAGE_OBJECT_LAYOUT_INTACT`  | S12.2 §13.2 | "Every package object on disk satisfies the closed-file-set + Merkle-root + state-consistency contract; loader rejects on any deviation." Already enforced at S12.2's loader; L0 promotion makes it constitutional. Verified mechanically by the Wave 8 property of the same name (20.1.3). |
| `NETWORK_DEFAULT_DENY_OUTBOUND` | S8.1 §3.4   | "Default-deny on all outbound network traffic; allowlist + per-app outbound manifests are the only escape. AI subjects are NEVER granted ALLOW_INTERNET." Carried forward from prior Wave (still queued — not promoted in Wave 6/7/8).                                                      |
| `PACKAGE_TRUST_CHAIN_BOUND`     | S11.1 §19   | "Every package's signing key chains to AIOS root in ≤ 3 hops; deeper chains rejected with FOREVER `TRUST_CHAIN_TOO_DEEP`. The signing chain is constitutional — no chain, no install." Already enforced at S11.1's install pipeline.                                                        |
| `ECOSYSTEM_HONESTY_DISCLOSURE`  | S12.1 §8    | "AIOS shall not present an `EcosystemHonestyClass` weaker than the runtime is verified to deliver. Honesty class disclosure is mandatory at install and at every operator-visible surface." Already enforced at S12.1's recipe registry.                                                    |

Candidate L0 invariants total after Wave 8 catalog: **6 candidates queued narrative-only**. Promotion path: a future single-purpose L0 sweep that authors the L0 invariant entries, increments the L0 invariant bundle version, and re-issues the bundle through recovery-mode per L1.1 `RecoveryMutableScope.INVARIANT_BUNDLE`.

### 20.6 Reconciliation

Total properties added in Wave 8: **6** (4 from S6.3, 1 from S13.1, 1 from S12.2).
New cumulative `PropertyType` enum count: 16 prior + 6 Wave 8 = **22 entries**.

Total primitives added in Wave 8: **3** (all from S8.4).
New cumulative primitive vocabulary count: 24 prior + 3 Wave 8 = **27 entries**.

Severity distribution of the 6 new properties: **constitutional 5** (`RECEIPT_SIGNATURE_VERIFIED`, `RECEIPT_REDACTION_VALID`, `RECEIPT_LINEAGE_DAG`, `RECEIPT_RETENTION_MATCHES_TYPE`, `AI_PROPOSAL_PIPELINE_INTACT`) / **operational 1** (`PACKAGE_OBJECT_LAYOUT_INTACT`) / **informational 0**.

### 20.7 Cross-spec impact note

- **New L0 invariants (audit-phase):** none promoted in Wave 8. Six candidates queued narrative-only — see §20.5.
- **New typed actions (S10.1 Wave 8):** none from this S2.4 sweep. The 6 typed actions queued by Wave 7 (S9.3's `kernel.build` / `kernel.refresh` and S12.1's four `app.*` actions) plus the Tier 2 typed-action surfaces (e.g. S15.x SGR transitions, S8.4 DNS/VPN actions, S8.5 firmware actions, S11.2 marketplace, S11.3 external integrations) remain queued for the next S10.1 catalog roll-up — out of scope for this contract.
- **Sources scanned with NO queued S2.4 contributions:** S9.2 (queues only S3.1 record types and the marker contract; no verification property), S14.1 (consumes S2.4's probe-error/verification-fail distinction; queues no new property), S0.3 (consumes existing S2.4 primitives; queues no new property), S15.1 / S15.2 / S15.3 (consume S2.4 primitive vocabulary for SGR health probes; queue no new property — S15.2 explicitly notes existing primitive names suffice), S13.2 (model router consumes S5.2 / S8.1 / S11.1; no S2.4 production), S12.3 (compatibility runtime consumes named primitives `process_alive` / `port_listening` / `unix_socket_listening` / `dbus_name_acquired` / `wayland_surface_visible` / `manifest_health_endpoint` from S2.4's existing closed catalog; no new property), S12.4 (compatibility knowledge; consumes S3.1 only), S7.6 (CLI renderer; consumes S7.2 + S7.3; no S2.4 production), S8.3 (hardware graph; queues only the L0 invariant candidate `HARDWARE_GRAPH_DRIFT_FOREVER` — caught at §20.5 — and S2.3 condition fields), S8.5 (firmware trust; queues a hardware-drift property via S8.3 and S5.4 NonOverridableClass review; no S2.4 property), S14.2 (telemetry pipeline; consumes S2.4's distinction; queues no S2.4 production), S11.2 (marketplace; consumes S5.3 / S5.4; no S2.4 production), S11.3 (external integrations; consumes S11.1 trust chain; no S2.4 production).
- **Composition note.** The receipt-integrity properties (20.1.1) compose naturally with the existing `EVIDENCE_LOG_APPEND_ONLY`, `EVIDENCE_HASH_CHAIN_INTACT`, and `STATUS_GRADE_CONSISTENT` properties (§7.1 base 9): a sealed receipt that passes the chain-intact predicate but fails `RECEIPT_SIGNATURE_VERIFIED` is a forgery surface that prior properties did not catch.

### 20.8 IDL reconciliation note

This section is a narrative declaration of the new closed enum entries and primitive messages. Full reconciliation against Appendix A (the consolidated proto IDL) is deferred to the next IDL roll-up, mirroring §17.1 / §18.7 / §19.6. No existing field number is changed; the additions are strictly additive.

## 21. See also

- [S0.1 Action Envelope + Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S3.1 Evidence Log](01_evidence_log.md)
- [S2.3 Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S4.1 Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S7.1 Surface Composition](../L7_Interaction_Renderers/01_surface_composition.md)
- [S7.2 Shared UI Schema](../L7_Interaction_Renderers/02_shared_ui_schema.md)
- [S7.3 Visual Language](../L7_Interaction_Renderers/03_visual_language.md)
- [S7.4 KDE Renderer](../L7_Interaction_Renderers/04_kde_renderer.md)
- [S7.5 Web Renderer](../L7_Interaction_Renderers/05_web_renderer.md)
- [S8.2 GPU Resource Model](../L8_Network_Hardware_Devices/05_gpu_resource_model.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

## Appendix A: Complete proto IDL

```proto
syntax = "proto3";
package aios.verification.v1alpha1;

import "google/protobuf/struct.proto";
import "google/protobuf/timestamp.proto";
import "google/protobuf/duration.proto";
import "google/protobuf/empty.proto";

// ─────────────────────────────────────────────────────────────────
// Intent
// ─────────────────────────────────────────────────────────────────

message VerificationIntent {
  string intent_id = 1;
  google.protobuf.Duration timeout = 2;
  oneof primitive {
    ServiceActiveIntent      service_active     = 10;
    ServiceInactiveIntent    service_inactive   = 11;
    PackageInstalledIntent   package_installed  = 12;
    PortOpenIntent           port_open          = 13;
    PortClosedIntent         port_closed        = 14;
    HttpOkIntent             http_ok            = 15;
    FileExistsIntent         file_exists        = 16;
    FileHashIntent           file_hash          = 17;
    RepoExistsIntent         repo_exists        = 18;
    AiosFsPointerIntent      aiosfs_pointer     = 19;
    PolicyDecisionIntent     policy_decision    = 20;
    EvidenceExistsIntent     evidence_exists    = 21;
    PropertyCheckIntent      property_check     = 22;
    Composition              composition        = 30;
  }
}

message ServiceActiveIntent     { string service = 1; }
message ServiceInactiveIntent   { string service = 1; }
message PackageInstalledIntent  { string package = 1; string version = 2; }
message PortOpenIntent          { string host = 1; uint32 port = 2; string protocol = 3; }
message PortClosedIntent        { string host = 1; uint32 port = 2; string protocol = 3; }
message HttpOkIntent {
  string url = 1;
  uint32 expected_status_min = 2;
  uint32 expected_status_max = 3;
  string expected_body_substring = 4;
}
message FileExistsIntent { string object_or_path = 1; }
message FileHashIntent   { string object_or_path = 1; string expected_hash_hex = 2; }
message RepoExistsIntent { string path_or_object = 1; }
message AiosFsPointerIntent {
  string object_id = 1;
  string pointer_kind = 2;
  string expected_version_id = 3;
}
message PolicyDecisionIntent {
  string policy_decision_id = 1;
  string expected_decision = 2;
}
message EvidenceExistsIntent { string receipt_id = 1; }

message PropertyCheckIntent {
  PropertyType type = 1;
  google.protobuf.Struct args = 2;
}

enum PropertyType {
  PROPERTY_TYPE_UNSPECIFIED         = 0;
  EVIDENCE_LOG_APPEND_ONLY          = 1;
  EVIDENCE_HASH_CHAIN_INTACT        = 2;
  AIOSFS_POINTER_HISTORY_ACYCLIC    = 3;
  POLICY_DEFAULT_DENY_HOLDS         = 4;
  POLICY_HARD_DENY_LIST_INTACT      = 5;
  AIOSFS_GC_REFCOUNT_CONSERVED      = 6;
  RECOVERY_PATH_BOOTABLE            = 7;
  PRIVACY_CLASS_MONOTONIC           = 8;
  TRANSACTION_LOG_REPLAYABLE        = 9;
}

// ─────────────────────────────────────────────────────────────────
// Composition
// ─────────────────────────────────────────────────────────────────

message Composition {
  oneof combinator {
    AllOf      all        = 1;
    AnyOf      any        = 2;
    NotOf      not        = 3;
    Eventually eventually = 4;
  }
}

message AllOf      { repeated VerificationIntent terms = 1; }
message AnyOf      { repeated VerificationIntent terms = 1; }
message NotOf      { VerificationIntent term = 1; }
message Eventually {
  VerificationIntent term = 1;
  google.protobuf.Duration max_duration = 2;
  google.protobuf.Duration interval     = 3;
}

// ─────────────────────────────────────────────────────────────────
// Result
// ─────────────────────────────────────────────────────────────────

message VerificationResult {
  string verification_id = 1;
  VerificationIntent intent = 2;
  VerificationStatus status = 3;
  string reason_code = 4;
  string reason_message = 5;
  google.protobuf.Struct observed = 6;
  google.protobuf.Timestamp verified_at = 7;
  google.protobuf.Duration probe_duration = 8;
  string evidence_receipt_id = 9;
  string action_id = 10;
  bool simulated = 11;
  repeated VerificationResult sub_results = 12;
}

enum VerificationStatus {
  VERIFICATION_STATUS_UNSPECIFIED = 0;
  VERIFICATION_PASSED      = 1;
  VERIFICATION_FAILED      = 2;
  VERIFICATION_TIMEOUT     = 3;
  VERIFICATION_PROBE_ERROR = 4;
  VERIFICATION_SKIPPED     = 5;
}

// ─────────────────────────────────────────────────────────────────
// Service
// ─────────────────────────────────────────────────────────────────

message RunVerificationRequest {
  string schema_version = 1;
  string action_id = 2;
  VerificationIntent intent = 3;
  string subject = 4;
  bool simulate = 5;
}

message ExplainResultRequest { string verification_id = 1; }

message ExplainResultResponse {
  VerificationResult result = 1;
  string narrative = 2;
  repeated string snapshot_ids = 3;
}

message VerificationEngineInfo {
  string engine_id = 1;
  repeated string supported_schema_versions = 2;
  string default_schema_version = 3;
  repeated string supported_primitives = 4;
  repeated string supported_property_types = 5;
  google.protobuf.Timestamp started_at = 6;
}

service VerificationEngine {
  rpc RunVerification(RunVerificationRequest) returns (VerificationResult);
  rpc ExplainResult(ExplainResultRequest) returns (ExplainResultResponse);
  rpc GetEngineInfo(google.protobuf.Empty) returns (VerificationEngineInfo);
}
```
