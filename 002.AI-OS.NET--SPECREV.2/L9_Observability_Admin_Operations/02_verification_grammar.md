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

## 17. See also

- [S0.1 Action Envelope + Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S3.1 Evidence Log](01_evidence_log.md)
- [S2.3 Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
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
