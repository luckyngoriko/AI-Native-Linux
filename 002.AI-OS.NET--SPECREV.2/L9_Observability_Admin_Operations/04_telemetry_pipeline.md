# Telemetry Pipeline (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists, structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Phase tag      | S14.2                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| Layer          | L9 Observability, Admin, Operations                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Schema package | `aios.telemetry.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Consumes       | L0 INV-005 (evidence append-only — telemetry is **not** evidence; the separation is enforced here), INV-014 (no proof, no completion — telemetry never substitutes for evidence at status promotion), INV-015 (evidence never contains secrets — extended in this contract to telemetry); S3.1 Evidence Log (distinct backbone — telemetry is for OPERATIONS, evidence is for AUDIT); S2.4 Verification Grammar (probe error vs verification fail distinction is mirrored); S14.1 Failure Handling (`FailureClass`, `DegradationLevel`, `BehaviorOnFailure` consumed as labels-by-enum); S0.1 Action Envelope (action lifecycle is observed but never used as a label); S4.1 Namespace Layout (closed `ScopeKind` is allowed as a label; `group_id` and `user_id` are forbidden); S8.1 Network Policy (telemetry exporters obey egress rules) |
| Produces       | the closed `TelemetrySignal` taxonomy; the closed `MetricKind`, `LogLevel`, `CardinalityBudget`, and `RetentionTier` enums (operational, distinct from S3.1 RetentionClass); the registration contract; the bounded-cardinality discipline (forbidden-as-label and allowed-as-label catalogs); the OpenTelemetry / Prometheus / Loki / eBPF integration map; the sampling policy; the redaction layer; the cardinality-breach handling table; the adversarial robustness profile; the performance contract; ten new evidence record types queued for S3.1 consolidation; three worked examples                                                                                                                                                                                                                                                |

## §1 Purpose

Up to this sub-spec, every other AIOS document has been free to assume "telemetry exists" without naming what telemetry is, what it is allowed to carry, where it lives, who reads it, and most importantly, **how it differs from evidence**. The two have been silently conflated in operator conversations and in code-review notes. That conflation is the single most dangerous category mistake available to AIOS:

- An evidence record (S3.1) is the audit witness. It is signed, hash-chained, append-only, FOREVER-retainable, never-redacted-below-a-floor, never-secrets-bearing, and policy-gated at append.
- A telemetry signal is an operational observation. It is sampled, redacted, droppable under load, time-series-shaped, low-cardinality-bound, and exporter-targeted.

If telemetry is allowed to drift toward evidence shape, the audit chain quality degrades because high-throughput dashboards inject high-cardinality and free-form payloads into the chain. If evidence is allowed to drift toward telemetry shape, the audit chain quality degrades because samples and drops invalidate the FOREVER guarantee. The two shapes must be physically separated.

This sub-spec closes the loop. It defines, in concrete enum-and-table form:

1. **What telemetry is** — a closed `TelemetrySignal` taxonomy with three values (METRIC, TRACE, LOG).
2. **The bounded-cardinality discipline** — the forbidden-as-label catalog and the allowed-as-label catalog, both closed.
3. **The backend map** — OpenTelemetry SDK as the standard, Prometheus as metric scrape, Loki as log aggregation, eBPF for kernel-side metric collection. Each backend's role is fixed.
4. **The retention tiering** — a closed `RetentionTier` enum (HOT_30D, WARM_90D, COLD_365D), distinct from S3.1's `RetentionClass`. Telemetry retention is operational; evidence retention is constitutional.
5. **The redaction layer** — telemetry mirrors S6.3-class evidence redaction discipline at the emission boundary. Telemetry can NEVER carry secrets, user content, or file contents.
6. **The sampling policy** — traces 1% by default, decision-at-trace-start, configurable per service.
7. **The cardinality-breach response** — AUTOMATIC_RETRY at registration; if persistent, AUTO_DEMOTE the offending label (not the metric); FOREVER `TELEMETRY_CARDINALITY_BREACH` evidence.
8. **The adversarial robustness profile** — cardinality-DoS, log-injection, clock-manipulation, exfiltration.
9. **The performance contract** — metric scrape p95 < 100 ms; log emit p95 < 1 ms; trace export p95 < 50 ms; eBPF probe overhead < 0.5% CPU.
10. **The evidence queue** — ten new record types for S3.1 consolidation, sized to the operational events that the telemetry pipeline itself produces.

After this spec, every reference in other specs to "we emit a counter for X", "we trace this path", or "we log Y" resolves to a single mechanical concept defined here, with the discipline that prevents the telemetry pipeline from itself becoming a leakage surface.

## §2 Scope

This spec **defines**:

1. The closed `TelemetrySignal` taxonomy (§3.1).
2. The closed `MetricKind` enum (§3.2).
3. The closed `LogLevel` enum (§3.3).
4. The closed `CardinalityBudget` enum (§3.4).
5. The closed `RetentionTier` enum (§3.5).
6. The registration contract — how a signal declares itself before emission (§4).
7. The bounded-cardinality discipline (§5).
8. The backend integration map (§6).
9. The sampling policy (§7).
10. The redaction layer (§8).
11. The cardinality-breach handling (§9).
12. The adversarial robustness profile (§10).
13. The performance contract (§11).
14. Three worked examples (§12).
15. Ten evidence record types queued for S3.1 consolidation (§13).
16. The acceptance criteria (§14).

This spec **does not** define:

- The evidence log shape — owned by S3.1. Telemetry record types queued in §13 cite S3.1's hash-chain and retention-class mechanics; this spec does not redefine them.
- The runbook content — owned by S14.1 §7 (failure-handling sub-spec). Telemetry alerts reference runbook paths but do not embed runbook bodies.
- The verification grammar — owned by S2.4. Telemetry consumes the probe-error vs verification-fail distinction (§10.2) but does not redefine it.
- The action envelope — owned by S0.1. Telemetry observes action throughput but never uses `action_id` as a label.
- The network egress rules for telemetry exporters — owned by S8.1. This spec states only that exporters obey those rules.
- The `RetentionClass` enum — owned by S3.1; that vocabulary is for evidence. This contract uses `RetentionTier` for OPERATIONS. The two enums are intentionally distinct types and live in different schema packages.

This spec is the **contract surface** that every other spec references when it says "we emit a metric", "we trace", or "we log".

## §3 Vocabulary (closed enums)

All vocabularies in this section are closed. Adding a value is a versioned spec change. Bundle load fails on unknown values.

### §3.1 `TelemetrySignal`

The top-level signal taxonomy. Every emitted observation in the AIOS telemetry pipeline is exactly one of these three values. There is no `OTHER`, no `EVENT`, no open-ended free-form bucket. **In particular, an evidence record is NOT a `TelemetrySignal`.** Evidence lives in S3.1; telemetry lives here. A signal that does not fit one of the three values below is an indication that the emitter is trying to use the telemetry pipeline for something that belongs in the evidence log, and the registration must be rejected.

```proto
syntax = "proto3";
package aios.telemetry.v1alpha1;

enum TelemetrySignal {
  TELEMETRY_SIGNAL_UNSPECIFIED = 0;
  METRIC                       = 1;   // numeric time series; aggregation by label
  TRACE                        = 2;   // distributed-trace span tree across services
  LOG                          = 3;   // structured log line with level + message + closed-label set
}
```

| Value    | One-line statement                                                                                                                                                                                                                                                           |
| -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `METRIC` | Numeric observation of a quantity over time (counter, gauge, histogram, summary). Aggregable along closed-enum labels. Scraped by Prometheus (§6.2). Subject to `CardinalityBudget` (§3.4). Sampled-by-default = false (metrics are not per-event; they are pre-aggregated). |
| `TRACE`  | One span in a distributed-trace tree. Carries trace_id (W3C) and span_id; stitches together across services. Sampled at 1% by default at trace start (§7). Exported via OpenTelemetry OTLP (§6.1). Drop-on-load is allowed.                                                  |
| `LOG`    | One structured log line. Carries `LogLevel` (§3.3), a fixed-format message template, and a closed-label set. Aggregated by Loki (§6.3). Drop-on-load is allowed below ERROR; ERROR and CRITICAL must reach the backend or fall through to in-memory ring buffer.             |

The closed enum has 3 values plus UNSPECIFIED. The list is a contract: no telemetry is allowed to be emitted outside it.

#### §3.1.1 `TelemetrySignal` is the operational perspective

`TelemetrySignal` is the operational-pipeline perspective. A METRIC is for "how is the system performing"; a TRACE is for "what path did this request take"; a LOG is for "what happened, in human terms, at this moment". An audit-grade record of "what happened" — the kind that operators sign and auditors review — is **not** a LOG. It is an evidence record (S3.1). The discriminator is: was the producer permitted by L4 to append to the evidence log for that record type? If yes → S3.1. If no → S14.2 LOG. The two surfaces never overlap by accident.

#### §3.1.2 Evidence is not telemetry; telemetry is not evidence

A signal observed by an operator dashboard is telemetry. A signal preserved as an audit witness is evidence. A signal that is both — e.g. "policy decision DENY rate" — is emitted to **both** pipelines independently:

- The evidence record `POLICY_DECISION` is appended to the S3.1 chain at decision time, with full payload, FOREVER-retainable retention class.
- A `aios_policy_decisions_total{decision, reason_code}` counter is incremented at the same emission site, with closed-enum labels only and no payload.

Both must succeed at the emission site for the action to proceed (per S14.1 §4.1 row 8: failure to append evidence is `RECOVERY_PENDING`). Failure to emit telemetry is **not** equivalent: the action proceeds, a `TELEMETRY_BACKEND_UNAVAILABLE` evidence record is queued for S3.1 (§13), and the telemetry pipeline transitions to its own degraded state. **Evidence cannot be redacted, sampled, or dropped under load. Telemetry can.**

### §3.2 `MetricKind`

Every registered metric declares exactly one kind. The closed set follows OpenTelemetry / Prometheus convention but is fixed here as part of the AIOS contract.

```proto
enum MetricKind {
  METRIC_KIND_UNSPECIFIED = 0;
  COUNTER                  = 1;   // monotonically non-decreasing accumulator
  GAUGE                    = 2;   // instantaneous value; can go up or down
  HISTOGRAM                = 3;   // bucketed distribution over a value range
  SUMMARY                  = 4;   // pre-computed quantile summary; rare; legacy
}
```

| Value       | Meaning                                                                                                                                                                                                                                                                |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `COUNTER`   | A monotonically non-decreasing value. Counter resets on process restart; the scrape consumer handles wraparound. Used for "events observed", "bytes processed", "decisions issued". Default cardinality budget: `MEDIUM_200`.                                          |
| `GAUGE`     | An instantaneous value at scrape time. Goes up and down. Used for "queue depth", "active subscribers", "degradation level numeric". Default cardinality budget: `SMALL_50`.                                                                                            |
| `HISTOGRAM` | A bucketed distribution over a value range. Buckets are pre-declared at registration; the scrape consumer reads bucket counts. Used for "latency distributions", "size distributions". Default cardinality budget: `MEDIUM_200` (bucket count is part of cardinality). |
| `SUMMARY`   | A pre-computed quantile summary. Discouraged; included for OpenTelemetry compatibility and migrations. New metrics should prefer `HISTOGRAM`. Default cardinality budget: `SMALL_50`.                                                                                  |

The closed enum has 4 values plus UNSPECIFIED.

### §3.3 `LogLevel`

Every emitted LOG signal carries exactly one level. The closed set is five values, ordered by severity:

```proto
enum LogLevel {
  LOG_LEVEL_UNSPECIFIED = 0;
  DEBUG                 = 1;   // verbose; off by default in production
  INFO                  = 2;   // normal operational milestones
  WARN                  = 3;   // recoverable anomaly observed
  ERROR                 = 4;   // operation failed; human attention may be needed
  CRITICAL              = 5;   // operation failed in a way that risks invariants
}
```

| Value      | Drop-on-load? | Default sampling | Backend                                                                                                     |
| ---------- | ------------- | ---------------- | ----------------------------------------------------------------------------------------------------------- |
| `DEBUG`    | yes           | 0% (off)         | Loki (when policy enables)                                                                                  |
| `INFO`     | yes           | 100%             | Loki                                                                                                        |
| `WARN`     | yes           | 100%             | Loki                                                                                                        |
| `ERROR`    | no            | 100%             | Loki + in-memory ring buffer                                                                                |
| `CRITICAL` | no            | 100%             | Loki + in-memory ring buffer + queued evidence record (`TELEMETRY_BACKEND_UNAVAILABLE` if Loki unreachable) |

#### §3.3.1 Why no `FATAL`

`FATAL` is intentionally absent. It is reserved for evidence-record severity (the FATAL outcome of a verification result, a tamper detection, an invariant violation). Reusing the same word for "this log line is super important" would silently mix the two domains and is prohibited. A signal severe enough to be FATAL is by definition evidence; it must be emitted as an evidence record (S3.1), not as a LOG. The S3.1 record-type vocabulary already covers the FATAL class (`TAMPER_DETECTED`, `CHAIN_INCONSISTENCY_DETECTED`, the FOREVER-retained classes from S14.1 §4). The `LogLevel` enum stops at `CRITICAL`.

The closed enum has 5 values plus UNSPECIFIED.

### §3.4 `CardinalityBudget`

Every registered metric and log declares exactly one cardinality budget. The budget is the upper bound on the number of distinct **label combinations** the signal is permitted to produce in a single host's retention window. Exceeding the budget triggers the cardinality-breach handling in §9.

```proto
enum CardinalityBudget {
  CARDINALITY_BUDGET_UNSPECIFIED = 0;
  SMALL_50                       = 1;   // ≤ 50 distinct combinations
  MEDIUM_200                     = 2;   // ≤ 200 distinct combinations
  LARGE_1000                     = 3;   // ≤ 1000 distinct combinations
  XLARGE_5000                    = 4;   // ≤ 5000 distinct combinations
  XXL_RESERVED                   = 5;   // reserved; not allocatable in normal mode
}
```

| Value          | Combinations | Typical use                                                                                                                                                                                |
| -------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `SMALL_50`     | ≤ 50         | Single-enum-label metrics. Example: `aios_degradation_level_active{level}` (6 levels × 1 host).                                                                                            |
| `MEDIUM_200`   | ≤ 200        | Two-enum-label metrics. Example: `aios_action_throughput_total{adapter_kind, result_kind}` (≈ 12 adapter kinds × 8 result kinds = 96).                                                     |
| `LARGE_1000`   | ≤ 1000       | Three-enum-label metrics or per-component breakdown. Example: `aios_policy_rule_evaluations_total{bundle_version, decision, reason_code}` (≈ 4 versions × 3 decisions × 50 reason codes).  |
| `XLARGE_5000`  | ≤ 5000       | Reserved for top-level observability dashboards aggregating across many enum dimensions. Requires explicit operator approval at registration; emits `TELEMETRY_PIPELINE_STARTED` evidence. |
| `XXL_RESERVED` | n/a          | Reserved for a future spec amendment. **Cannot be allocated in normal mode.** Attempting to register a signal at XXL_RESERVED budget is rejected with `BUDGET_NOT_ALLOCATABLE`.            |

The closed enum has 5 values plus UNSPECIFIED.

#### §3.4.1 Why budgets are mandatory at registration

Time-series databases scale linearly with the number of distinct label combinations, not with the number of observations. A counter incremented 10 million times with 50 distinct label combinations is cheap; a counter incremented 10 thousand times with 10 thousand distinct label combinations is a pathology. Without a registration-time budget, a single misconfigured emission site can blow up the cost of the entire telemetry storage tier. The budget is therefore mandatory, declared at registration, and enforced at emit time.

### §3.5 `RetentionTier`

Every registered telemetry signal declares exactly one retention tier. Tiers govern how long the signal's data is kept on the operational tier before it is dropped (not migrated, not summarized — **dropped**, because telemetry is not evidence). The tiers are operational and bear no relationship to the S3.1 `RetentionClass` enum, which is for evidence and is the constitutional retention surface.

```proto
enum RetentionTier {
  RETENTION_TIER_UNSPECIFIED = 0;
  HOT_30D                    = 1;   // 0–30 days; SSD; full-resolution; fast query
  WARM_90D                   = 2;   // 30–90 days; spinning disk or compressed; reduced resolution
  COLD_365D                  = 3;   // 90–365 days; object storage; on-demand fetch; slow query
}
```

| Tier        | Default retention | Storage                                                             | Query latency | Default applicability                            |
| ----------- | ----------------- | ------------------------------------------------------------------- | ------------- | ------------------------------------------------ |
| `HOT_30D`   | 30 days           | SSD; full resolution                                                | < 100 ms      | All metrics by default; ERROR/CRITICAL logs      |
| `WARM_90D`  | 90 days           | Disk; compressed; downsampled to 1-minute resolution beyond 30 days | < 1 s         | Selected metrics with operator dashboards        |
| `COLD_365D` | 365 days          | Object storage                                                      | seconds       | Selected metrics for long-term capacity planning |

Beyond `COLD_365D`'s horizon, telemetry is **dropped**. Long-term retention of operational state is not a telemetry concern; long-term retention of audit-relevant events is the evidence log's concern (S3.1).

#### §3.5.1 Why `RetentionTier` is distinct from `RetentionClass`

S3.1's `RetentionClass` (`STANDARD_24M`, `EXTENDED_60M`, `FOREVER`) is for evidence records. It is a **constitutional** surface: a record marked `FOREVER` is preserved by the chain mechanics for the lifetime of the AIOS instance, never compacted away beyond the tombstone level. Telemetry, in contrast, is **operational**: high-volume, sampled, redacted, droppable. Mixing the two enums would imply that telemetry can be kept FOREVER (it cannot — that would push high-cardinality time-series data into the FOREVER-retained constitutional layer, defeating both cost containment and the audit-trail clarity) or that evidence can be kept for HOT_30D (it cannot — a 30-day retention horizon on a denial decision violates INV-005 and S14.1 row 1). The two enums are intentionally separate types in separate schema packages.

The closed enum has 3 values plus UNSPECIFIED.

The closed enum has 3 retention tiers; total enum count for this spec is 5 closed enums (TelemetrySignal, MetricKind, LogLevel, CardinalityBudget, RetentionTier), summing to 20 closed values plus 5 UNSPECIFIEDs.

## §4 Registration contract

Every telemetry signal must be **registered** before it can be emitted. Registration is the moment the cardinality discipline is enforced, the redaction layer is bound, and the retention tier is declared. Emitting an unregistered signal is rejected.

### §4.1 The registration record

```proto
message TelemetryRegistration {
  string signal_id              = 1;    // canonical: "aios.<layer>.<component>.<name>"
  TelemetrySignal kind          = 2;    // METRIC / TRACE / LOG
  MetricKind metric_kind        = 3;    // populated only when kind == METRIC
  LogLevel default_log_level    = 4;    // populated only when kind == LOG
  CardinalityBudget budget      = 5;
  RetentionTier retention_tier  = 6;
  repeated string allowed_labels = 7;   // closed list of label names, drawn from §5.2
  string emitting_layer         = 8;    // "L0".."L10"; closed enum value as string
  string emitting_component     = 9;    // canonical component name; closed catalog
  string description            = 10;   // operator-facing prose; redacted of subject content
  string redaction_profile      = 11;   // "default" | "strict"; "debug_capture" forbidden in §8.4
  google.protobuf.Timestamp registered_at = 12;
  string registrar_subject_id   = 13;   // L4 canonical subject of the registrar
}
```

### §4.2 Registration lifecycle

```text
  caller submits TelemetryRegistration
            │
            v
  §5 cardinality validation (forbidden labels, allowed labels, budget plausibility)
            │
            v
  §8 redaction-profile validation (debug_capture rejected unless §8.4 path)
            │
            v
  §6 backend mapping (which backend will receive this signal)
            │
            v
  registration accepted → signal_id reserved → emission permitted
            │
            v
  TELEMETRY_PIPELINE_STARTED evidence (STANDARD_24M)
```

### §4.3 Re-registration

A signal can be re-registered with a **broader** cardinality budget (e.g. `MEDIUM_200 → LARGE_1000`) but **never** with a narrower budget (the existing time-series would invalidate). Re-registration with a narrower budget is rejected; the operator must rename the signal. Re-registration emits `TELEMETRY_PIPELINE_STARTED` again with the new budget.

### §4.4 Unregistered emission

An emit call against an unregistered `signal_id` is rejected with `EMITTER_UNREGISTERED` and emits a single `TELEMETRY_BACKEND_DEGRADED` evidence record (rate-limited to one per minute per emitter; §13). The data point is dropped. There is no implicit registration — the registration step is the discipline boundary.

## §5 Bounded-cardinality discipline

This is the heart of the spec. A telemetry pipeline that allows arbitrary high-cardinality labels is a denial-of-service surface against the operator's storage tier and a leakage surface against subject identifiers. Both must be bounded mechanically.

### §5.1 The forbidden-as-label catalog

The following identifier kinds **MUST NEVER** appear as labels on any METRIC, TRACE attribute, or LOG label:

| Forbidden label             | Why                                                                                                                                    |
| --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `subject_id`                | Per-subject cardinality is unbounded (millions of identities possible); leaks identity into operational tier.                          |
| `session_id`                | Per-session cardinality is unbounded; sessions are short-lived, generating high churn.                                                 |
| `action_id` (S0.1 ULIDs)    | Per-action cardinality is unbounded (every action has a unique id); pushes total combinations past any budget.                         |
| `receipt_id` (S3.1 ULIDs)   | Same as above; leaks evidence-chain identifiers into telemetry tier.                                                                   |
| `object_id` (AIOS-FS ULIDs) | Per-object cardinality is unbounded.                                                                                                   |
| `user_id`                   | Per-user cardinality grows with the operator's user base; leaks PII into telemetry.                                                    |
| `group_id`                  | Per-group cardinality grows with the operator's deployment; leaks group identity into telemetry. Uses `ScopeKind` enum instead (§5.2). |
| `agent_id`                  | Per-agent cardinality is unbounded; leaks AI-subject identifier into telemetry.                                                        |
| `package_id`                | Per-package cardinality grows with the marketplace; leaks installed-package identity into telemetry.                                   |
| `surface_id` (S7.1 ULIDs)   | Per-surface cardinality is unbounded; surfaces are short-lived.                                                                        |
| `device_id`                 | Per-device cardinality grows with the hardware graph; leaks hardware identity into telemetry.                                          |
| `fqdn`                      | Per-FQDN cardinality is unbounded (any string); allows DNS-based label injection (§10.5).                                              |
| `ip_address`                | Per-IP cardinality is unbounded; leaks endpoint identity; subject to rapid churn (DHCP, NAT).                                          |

A registration that lists any of these as `allowed_labels` is rejected with `FORBIDDEN_LABEL_ON_REGISTRATION` and emits a `TELEMETRY_PIPELINE_STARTED` record with `outcome = REJECTED`. There is no per-deployment override.

#### §5.1.1 Why the forbidden list is closed

The forbidden list is a closed enum of rules — adding a forbidden label requires a versioned spec amendment. Removing a forbidden label is **prohibited** in normal mode (it can only happen via a recovery-mode invariant-bundle update by a HUMAN_USER subject). The closed list is the spec's defense against the "let's just add this one identifier as a label, it'll be fine" pattern that destroys observability cost containment in every system that allows it.

### §5.2 The allowed-as-label catalog

The following are the only label kinds permitted on AIOS telemetry. Any label outside this catalog must be reduced to a value within the catalog before emission.

| Allowed label kind    | Source                                                                                                                             | Closed-enum cardinality |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ----------------------- |
| Closed enum value     | Any closed enum from any AIOS spec (e.g. `Decision`, `VerificationStatus`, `FailureClass`, `LogLevel`)                             | Per-enum, ≤ 50 typical  |
| `layer`               | `"L0"` … `"L10"`                                                                                                                   | 11                      |
| `component`           | Closed catalog of component names per layer (e.g. `"policy_kernel"`, `"capability_runtime"`, `"vault_broker"`)                     | ≤ 50                    |
| `result_kind`         | Closed enum: `SUCCESS`, `FAILURE`, `TIMEOUT`, `DENIED`, `RETRY`                                                                    | 5                       |
| `error_code`          | Closed enum from the spec that owns the metric (e.g. `NetworkPolicyErrorCode`, `PolicyDecisionReasonCode`) — must be a closed enum | per-enum, ≤ 100 typical |
| `scope`               | S4.1 `ScopeKind` enum: `SYSTEM` / `GROUP` / `USER`                                                                                 | 3                       |
| `decision`            | S2.3 `Decision` enum: `ALLOW` / `REQUIRE_APPROVAL` / `DENY`                                                                        | 3                       |
| `degradation_level`   | S14.1 `DegradationLevel` enum                                                                                                      | 6                       |
| `failure_class`       | S14.1 `FailureClass` enum                                                                                                          | 15                      |
| `behavior_on_failure` | S14.1 `BehaviorOnFailure` enum                                                                                                     | 5                       |
| `signal_kind`         | S14.2 `TelemetrySignal` enum (this spec)                                                                                           | 3                       |
| `metric_kind`         | S14.2 `MetricKind` enum (this spec)                                                                                                | 4                       |
| `log_level`           | S14.2 `LogLevel` enum (this spec)                                                                                                  | 5                       |
| `retention_tier`      | S14.2 `RetentionTier` enum (this spec)                                                                                             | 3                       |
| `cardinality_budget`  | S14.2 `CardinalityBudget` enum (this spec)                                                                                         | 5                       |
| `is_ai`               | Boolean derived from L4 identity (`true` / `false`)                                                                                | 2                       |
| `recovery_mode`       | Boolean derived from L4 identity (`true` / `false`)                                                                                | 2                       |
| `simulated`           | Boolean from S0.1 envelope                                                                                                         | 2                       |
| `outcome`             | Closed enum: `OK`, `REJECTED`, `DROPPED`, `RATE_LIMITED`                                                                           | 4                       |
| `tier` (telemetry)    | `RetentionTier` (this spec)                                                                                                        | 3                       |
| `seal_reason`         | Closed enum from S3.1                                                                                                              | 4                       |

A label drawn from a closed enum is the only category permitted. Free-form strings, numeric ids, and continuous-value labels are all forbidden. The result is that the **maximum** cardinality of any single signal is bounded by the product of the cardinalities of its labels, and that product is computable at registration time.

#### §5.2.1 The cardinality plausibility check

At registration, the engine computes:

```text
expected_max_cardinality = product over label in allowed_labels of cardinality(label)
```

If `expected_max_cardinality > budget.upper_bound`, the registration is rejected with `BUDGET_TOO_SMALL_FOR_LABELS`. The registrar must either choose a larger budget (within the allocatable set; `XXL_RESERVED` is not allocatable) or remove labels from the registration.

### §5.3 Cardinality observation at runtime

The engine maintains a per-signal observed-distinct-combination counter. At every emission, the counter is updated for the (hashed) label tuple. If the counter exceeds the registered `budget.upper_bound`, the cardinality-breach handling (§9) is triggered.

### §5.4 The "telemetry as label-store" anti-pattern

A common antipattern is to use telemetry as a label-store: emit a metric with a label that is "just a small enum, but each value is actually a free-form identifier". Two examples that look innocent and are forbidden:

- `aios_app_starts_total{app_name}` — `app_name` is a free-form string from the marketplace catalog, not a closed enum. Forbidden.
- `aios_user_login_total{username}` — `username` is per-user, not closed. Forbidden.

The corrected forms are:

- `aios_app_starts_total{layer, app_origin}` where `app_origin` is a closed enum (`SYSTEM_BUNDLE`, `LOCAL_BUILD`, `MARKETPLACE`, `SIDELOADED`).
- `aios_subject_login_total{is_ai, scope, result_kind}`.

### §5.5 Trace span attributes follow the same rules

A trace span has `attributes`, which are the trace equivalent of metric labels. The forbidden-as-label catalog (§5.1) and the allowed-as-label catalog (§5.2) **apply identically** to span attributes. A span that includes `subject_id` as an attribute is rejected at the SDK boundary and emits `TELEMETRY_REDACTION_FAILED` (FOREVER; §13).

### §5.6 Log labels follow the same rules

A log line's structured-label set follows the same forbidden/allowed catalogs. The free-form **message** body is permitted (it goes through the redaction layer; §8) but **labels** must be closed-enum values only.

## §6 Backend integration map

AIOS uses a fixed set of backends. Each backend has a defined role; the boundary between backends is part of the contract.

### §6.1 OpenTelemetry SDK as the standard

The OpenTelemetry SDK is the **only** library through which AIOS components emit telemetry. Direct emission to Prometheus / Loki / eBPF is forbidden — the SDK is the abstraction layer that:

1. Enforces registration before emission (§4).
2. Applies the redaction layer (§8) at emission boundary.
3. Routes signals to the correct backend per kind (METRIC → Prometheus, TRACE → OTLP collector, LOG → Loki).
4. Implements sampling (§7).
5. Reports cardinality observations to the registry (§5.3).

The SDK is a per-process library; every Rust execution-side component links against `opentelemetry-rust` 0.20+; every Python cognitive-side component links against `opentelemetry-python` 1.20+. Both produce the same wire format (OTLP).

### §6.2 Prometheus as metric scrape

Metrics are exposed via Prometheus's pull-based scrape model. Each AIOS process exposes a `/metrics` endpoint on `127.0.0.1:<process_port>`; a single host-local Prometheus instance scrapes all processes. The Prometheus instance is a userspace service in the L9 telemetry namespace, **not** part of the recovery-safe path (per INV-001 — recovery does not require telemetry).

| Property         | Value                                                                                                                                                      |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Scrape interval  | 15 s (configurable; minimum 5 s)                                                                                                                           |
| Scrape p95       | < 100 ms (§11)                                                                                                                                             |
| Endpoint         | `127.0.0.1:<process_port>/metrics`                                                                                                                         |
| Network exposure | Loopback only (per INV-006); LAN exposure of `/metrics` requires the same `WEB_LAN_EXPOSURE_GRANTED` evidence path as any other web exposure (S7.5 / S8.1) |
| Storage          | Prometheus TSDB on operator-configured local disk                                                                                                          |
| Retention        | per-signal `RetentionTier` (§3.5)                                                                                                                          |
| Federation       | Optional; multi-host federation deferred to a future operational sub-spec                                                                                  |

### §6.3 Loki as log aggregation

Logs are pushed to Loki via its push API. Each AIOS process buffers structured logs and pushes them in batches (default 5 s flush interval, or 1 MB buffer, whichever first).

| Property                  | Value                                                                                                                                                                                    |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Batch flush interval      | 5 s (configurable; minimum 1 s)                                                                                                                                                          |
| Batch max size            | 1 MB                                                                                                                                                                                     |
| Log emit p95 (in-process) | < 1 ms (§11)                                                                                                                                                                             |
| Endpoint                  | `127.0.0.1:3100/loki/api/v1/push` (loopback)                                                                                                                                             |
| Storage                   | Loki on operator-configured local disk; chunk-based                                                                                                                                      |
| Retention                 | per-signal `RetentionTier` (§3.5)                                                                                                                                                        |
| Network exposure          | Loopback only by default (per INV-006)                                                                                                                                                   |
| Backpressure              | If Loki is unreachable, ERROR and CRITICAL logs go to the in-memory ring buffer (per §3.3 row); INFO/WARN/DEBUG are dropped with `TELEMETRY_BACKEND_UNAVAILABLE` evidence (rate-limited) |

### §6.4 eBPF for kernel-side metric collection

Kernel-side metrics (syscall counts, scheduler latencies, I/O rates) are collected via eBPF probes. AIOS uses a fixed set of probe templates — operators cannot install arbitrary probes through the telemetry contract.

| Property                 | Value                                                                                                                                                                                              |
| ------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Probe loader             | Privileged AIOS service running as `_system` subject; AI subjects cannot load probes (per INV-002, INV-013)                                                                                        |
| Probe templates          | Closed catalog: `syscall_count`, `block_io_latency`, `scheduler_runqueue_depth`, `tcp_retransmit_count`, `oom_kill_count`, `network_drop_count`. **No probe outside this catalog** is installable. |
| Probe overhead budget    | < 0.5% CPU host-wide (§11)                                                                                                                                                                         |
| Probe load evidence      | `TELEMETRY_EBPF_PROBE_LOADED` (STANDARD_24M; §13)                                                                                                                                                  |
| Probe rejection evidence | `TELEMETRY_EBPF_PROBE_REJECTED` (FOREVER; §13)                                                                                                                                                     |
| Output                   | Probe values are emitted as `METRIC` signals through the OpenTelemetry SDK (§6.1); they obey the same registration, cardinality, and retention disciplines as any other metric.                    |

#### §6.4.1 Why eBPF probes are restricted to a closed catalog

eBPF probes have kernel-side privilege. An attacker who can install a probe of arbitrary shape can read kernel memory, observe other process syscalls, and exfiltrate via the metric channel. The closed-catalog discipline limits probe authorship to a vetted set of templates whose data flow is auditable. Probe authorship outside the catalog requires a versioned spec amendment.

### §6.5 The four backends are physically separate

| Backend           | Process                           | Port              | Authority                                                  |
| ----------------- | --------------------------------- | ----------------- | ---------------------------------------------------------- |
| OpenTelemetry     | per-process SDK                   | n/a (in-process)  | Per-process identity                                       |
| Prometheus        | `prometheus` system service       | 127.0.0.1:9090    | `_system` subject; HUMAN_USER can read; AI cannot read raw |
| Loki              | `loki` system service             | 127.0.0.1:3100    | `_system` subject; HUMAN_USER can read; AI cannot read raw |
| eBPF probe loader | `aios-ebpf-loader` system service | n/a (kernel-side) | `_system` subject only; AI hard-denied                     |

The four backends do not cross-call. Prometheus does not push to Loki; Loki does not pull from Prometheus; eBPF does not write to Loki. The cross-backend correlation is done at the **dashboard** layer, not at the telemetry-pipeline layer.

## §7 Sampling policy

Sampling is the discipline that bounds telemetry volume to a constant fraction of the underlying event rate. Without sampling, an order-of-magnitude burst of work produces an order-of-magnitude burst of telemetry, exhausting backend ingest.

### §7.1 Trace sampling

Traces are sampled at **1% by default**. The sampling decision is made **at trace start**, never per-span. Once a trace is sampled-in, every span in the trace is recorded; once a trace is sampled-out, no span in the trace is recorded. This is the OpenTelemetry "head sampling" discipline.

#### §7.1.1 Per-service override

The sampling rate is configurable per service via the registration `description` field's `sampling_override`. Allowed values: `0.001` (0.1%), `0.01` (1%, default), `0.05` (5%), `0.10` (10%), `0.50` (50%), `1.00` (always-on; only with FOREVER-evidence justification). Other values are rejected at registration. Changes emit `TELEMETRY_SAMPLING_RATE_ADJUSTED` (STANDARD_24M; §13).

#### §7.1.2 Why 1% default

A 1% default is a defensible compromise for AIOS workloads:

- High enough to capture statistically representative samples of normal-mode operation (≈ 1 of every 100 operations).
- Low enough to keep trace storage bounded under the Prometheus-class budgets.
- Standard in the OpenTelemetry community; AIOS does not invent a custom default.

For operations where the **exemplar** of failure matters more than the sample rate (e.g. a rare invariant-violation path), the trace is **forced sampled-in** at the emission site by setting the sampling-decision attribute on the root span. This is allowed only for FOREVER-class events.

### §7.2 Metric sampling

Metrics are **not sampled**. A counter increments on every observed event; sampling a counter would lose the absolute count. Bounded volume comes from cardinality discipline (§5), not sampling.

### §7.3 Log sampling

Logs are sampled per `LogLevel`:

- `DEBUG` is off by default (sampled at 0%); explicit policy approval required to enable.
- `INFO`, `WARN`, `ERROR`, `CRITICAL` are 100% emitted at the registration point. Backpressure-driven drops at the backend boundary are observed via `TELEMETRY_BACKEND_DEGRADED` evidence; they are not sampling.

### §7.4 Sampling decisions are evidence

Every change to a sampling rate emits `TELEMETRY_SAMPLING_RATE_ADJUSTED` with the previous and new rate, the registrar subject, and the reason code. Operators can audit sampling drift over time.

## §8 Redaction layer

This is the second hard discipline. **Telemetry can NEVER contain secrets, user content, or file contents.** The redaction layer mirrors the S6.3-class evidence-redaction discipline at the emission boundary.

### §8.1 The forbidden-content catalog

The following content kinds **MUST NEVER** appear in any METRIC label, TRACE attribute, LOG label, or LOG message:

| Forbidden content          | Why                                                                                                       |
| -------------------------- | --------------------------------------------------------------------------------------------------------- |
| Secret material            | Per INV-015 / INV-018; secrets in telemetry would defeat the vault-broker contract.                       |
| User content               | User-typed prompts, messages, file bodies, clipboard content. Privacy ceiling violation.                  |
| File contents              | Bytes from any AIOS-FS object, regardless of object class.                                                |
| Authentication tokens      | OAuth tokens, JWTs, session cookies, API keys.                                                            |
| Cryptographic key material | Public keys are allowed (they are not secret); private keys, symmetric keys, and HMAC keys are forbidden. |
| Network packet payloads    | Beyond the source-port/dest-port and closed-enum protocol kind.                                           |
| Personal identifiers       | Email, phone, government id, full name, address.                                                          |

### §8.2 Where redaction is applied

Redaction is applied **at the emission boundary**, inside the OpenTelemetry SDK. The SDK rejects any emission that fails the redaction check. This is a hard reject — the data point is dropped, and a `TELEMETRY_REDACTION_FAILED` evidence record (FOREVER; §13) is appended to the S3.1 chain. The data point is never stored in the operational tier and never reaches Prometheus / Loki / OTLP.

#### §8.2.1 The pattern catalog

The redaction layer applies a closed pattern catalog, mirrored from S1.1 §17.2.6 (secret-shaped substring detection):

- PEM block headers (`-----BEGIN PRIVATE KEY-----`, etc.).
- High-entropy strings ≥ 32 chars (likely secrets).
- Patterns matching common token formats (JWT shape: three Base64URL segments separated by `.`; AWS-key shape; OpenAI-key shape; etc.).
- File paths with content (a path is allowed; a path followed by file body is not).

A match in any field rejects the emission with `REDACTION_PATTERN_MATCHED` and emits `TELEMETRY_REDACTION_FAILED`.

### §8.3 Redaction is content-only, not metadata-only

The redaction discipline applies to the **payload** of telemetry. Metadata — closed-enum labels, the message template (without filled-in fields), the timestamp, the layer/component — is not subject to redaction (they are already known not to carry secrets, by construction). What is redacted is anything that flowed in from a subject-controlled source: argument values to log messages, filled-in template fields, dynamic tag values.

### §8.4 No `debug_capture` profile

S3.1 §14 defines a `debug_capture` redaction profile for evidence that disables most redaction (only secrets are scrubbed). **Telemetry has no equivalent.** The two redaction profiles available to telemetry are:

- `default` — all redaction patterns applied; ordinary operation.
- `strict` — `default` plus PII heuristics; recommended for any signal whose payload could contain user-identifying free-form text.

`debug_capture` is forbidden for telemetry: enabling it would push secret-shaped content into a high-volume operational tier where dashboards, queries, and exports could surface it. There is no operator workflow that justifies that risk.

A registration that requests `debug_capture` is rejected with `TELEMETRY_DEBUG_CAPTURE_FORBIDDEN` and emits `TELEMETRY_REDACTION_FAILED`.

### §8.5 Redaction failures are evidence

Every redaction failure emits `TELEMETRY_REDACTION_FAILED` (FOREVER) with:

- `signal_id` of the registration whose emission was blocked.
- `pattern_class` (PEM block / high-entropy / JWT / AWS-key / OpenAI-key / generic-token).
- `emitter_subject_id` (the subject identity, redacted to scope+is_ai if subject-id itself is high-cardinality at the receiver).
- `dropped_at` timestamp.

Operators can audit which signals are repeatedly emitting redaction-rejected payloads — that is itself an indication of a buggy emission site.

## §9 Cardinality breach handling

When a signal exceeds its declared `CardinalityBudget` upper bound, the engine responds with a disciplined sequence rather than an unbounded reject.

### §9.1 The handling sequence

```text
  observed_combinations exceeds budget.upper_bound
            │
            v
  Step 1: AUTOMATIC_RETRY at the registration level
          (the engine sleeps 60 s and re-evaluates;
           transient bursts may resolve)
            │
            v
  if breach persists after Step 1:
            │
            v
  Step 2: AUTO_DEMOTE the offending label
          (NOT the metric)
            │
            v
  Step 3: emit TELEMETRY_CARDINALITY_BREACH evidence (FOREVER)
            │
            v
  Step 4: continue emitting the metric with the demoted label set
```

### §9.2 Why demote the label, not the metric

Demoting the **metric** would be a denial-of-service against the operator's dashboards: a single misconfigured emitter could turn off an entire metric the operator depends on. Demoting the **offending label** preserves the metric while removing the cardinality explosion.

The "offending label" is identified by:

- Computing per-label cardinality contribution (the number of distinct values observed for that label alone).
- Selecting the label with the highest contribution.
- Replacing its emission value with the constant string `"_demoted"` for the rest of the retention window.

Operators see the demotion in the time-series: a spike of new label values is followed by a flat line at `_demoted`. They can investigate the emission site and re-register with a corrected label set.

### §9.3 The breach evidence record

`TELEMETRY_CARDINALITY_BREACH` (FOREVER; §13) carries:

- `signal_id`.
- `budget` (the registered value).
- `observed_combinations` (the count at breach time).
- `demoted_label` (the chosen offender).
- `breach_first_seen_at`.

### §9.4 Re-registration after breach

The operator can re-register the signal with a larger budget (per §4.3), clearing the demoted label set. Until re-registration, the demoted label remains demoted for that signal. The re-registration attempt that requests `XXL_RESERVED` budget — which would in principle accommodate any cardinality — is rejected because `XXL_RESERVED` is not allocatable in normal mode (§3.4 row).

### §9.5 No silent suppression

The cardinality-breach handling never silently drops data points without evidence. Every breach emits FOREVER evidence; every demotion is observable in the time-series. This is the telemetry analog of S14.1 §8.3 (no silent suppression).

## §10 Adversarial robustness

The telemetry pipeline is itself an attack surface. This section enumerates the threat surface and the mitigations.

### §10.1 Threat — cardinality DoS via subject-id-as-label

**Attack:** an adversary instruments code to emit a metric with `subject_id` as a label, causing per-subject time-series explosion that exhausts Prometheus storage.

**Mitigation:** the forbidden-as-label catalog (§5.1) lists `subject_id` explicitly. Registration is rejected with `FORBIDDEN_LABEL_ON_REGISTRATION`. The signal cannot be emitted. No data points reach the storage tier. The defense is at the registration boundary, not at the storage boundary — by the time data reaches storage, the cost has already been incurred.

### §10.2 Threat — log injection via crafted user input

**Attack:** an adversary submits user input containing log-format escape sequences (newlines, ANSI escapes, structured-log parser confusables) that, when logged verbatim, fake a separate log line — possibly faking a CRITICAL line that triggers operator alerts.

**Mitigation:** the SDK applies log-line escape sanitization at the emission boundary. Every dynamically-filled message field is passed through a sanitizer that:

- Replaces `\n` and `\r` with the literal characters `\n` and `\r` (escaped, not separator).
- Strips ANSI escape sequences (the `\x1b[` prefix and trailing terminator).
- Strips control characters in the 0x00–0x1F range (except `\t` which is preserved as `\t`).
- Truncates dynamic fields to 4096 bytes (oversized fields are likely dump-style attacks).

A sanitizer rejection emits `TELEMETRY_LOG_INJECTION_DETECTED` (FOREVER; §13) with the originating signal_id and the offending field name (not the offending content — including the content would itself be a leak vector).

#### §10.2.1 Probe-error vs verification-fail mirror

The S2.4 distinction between verification probe error (the probe broke) and verification fail (the predicate failed) is mirrored here as: a sanitizer rejection (the sanitizer broke / received malformed input) is **not** the same as a redaction rejection (the content matched a forbidden pattern). The two emit different evidence record types and follow different operator workflows. Conflating them would mask real attacks behind dashboard noise.

### §10.3 Threat — clock manipulation via crafted OpenTelemetry timestamps

**Attack:** an adversary submits OpenTelemetry data with crafted timestamps to fake timeline ordering, place events in the past or future, or reorder traces around a security-relevant event.

**Mitigation:** the OpenTelemetry SDK uses the **host monotonic clock anchored to TAI64N**, not the emitter's claim. The discipline is the same as S6.3's evidence-receipt timestamping (the receipt schema sub-spec at L0). Specifically:

- The emitter's claimed timestamp is recorded as `client_timestamp` in the trace span attributes (and is itself subject to redaction if it carries content).
- The canonical timestamp on the signal is the SDK's read of the constitutional clock, anchored to TAI64N per S6.3.
- Drift between claimed and canonical exceeding ±5 s emits a `TIME_DRIFT_DETECTED` evidence record (S14.1 §4.1 row 29), which is the same constitutional time-drift mechanism that protects evidence ordering.
- Sampling decisions (§7) use the canonical timestamp, never the claimed timestamp.

### §10.4 Threat — telemetry exfiltration of user data

**Attack:** an adversary instruments code to emit a metric or log line that includes user content as a label or message field, with the intent that the dashboard or external exporter surfaces it.

**Mitigation:** the redaction layer (§8) rejects emissions whose payload matches the secret-pattern catalog or the PII catalog under `strict` profile. The `debug_capture` profile is forbidden for telemetry (§8.4), eliminating the operator-toggleable bypass. Rejection emits `TELEMETRY_REDACTION_FAILED` (FOREVER), and the data is dropped before it reaches the storage tier.

### §10.5 Threat — DNS-based label injection (FQDN-as-label bypass)

**Attack:** an adversary registers a metric with a closed-enum-looking field that is actually FQDN-derived, hoping the registration check accepts it. They then control DNS, returning crafted FQDNs to inflate cardinality.

**Mitigation:** `fqdn` is in the forbidden-as-label catalog (§5.1). The registration check verifies that every label name in `allowed_labels` matches the closed catalog, not a free-form string. A registration that lists `target_host` (which would be accepted because it is not literally `fqdn`) but whose values are FQDNs is caught at runtime by the cardinality observer (§5.3): the observed-combinations grows past the budget, breach handling triggers (§9), and the offending label is demoted.

#### §10.5.1 Why two layers of defense

The first layer (registration check) catches labels that are forbidden by name. The second layer (runtime cardinality observation) catches labels that pass the name check but whose value distribution reveals the underlying free-form-ness. Adversaries have to defeat both layers to mount a cardinality attack.

### §10.6 Threat — ring-buffer exhaustion during backend outage

**Attack:** an adversary deliberately fills the in-memory ring buffer (used for ERROR/CRITICAL when Loki is unreachable) by triggering high-volume CRITICAL emissions, exhausting host memory.

**Mitigation:** the ring buffer is bounded (default 16 MB; configurable). When full, oldest entries are dropped with a single `TELEMETRY_BACKEND_DEGRADED` evidence record (rate-limited to one per minute per buffer). The buffer is a degraded mode, not a primary path; the primary path is Loki, and the system's response to persistent Loki unavailability is operator notification, not unbounded buffering.

### §10.7 Threat — telemetry as covert channel

**Attack:** an adversary uses the timing or count of a telemetry emission to encode information that exfiltrates through the operator's dashboard.

**Mitigation:** this attack is constrained but not eliminated. AIOS chooses not to defend against the most subtle covert-channel attacks at the telemetry layer (defending fully would require sampling and rate-limiting that defeat operational utility). Instead, AIOS bounds the available bandwidth: closed-enum labels, sampling, redaction, retention drops. Operators auditing for covert-channel misuse have access to the full registration set (§4.1) and can verify that no unexpected signals are registered.

## §11 Performance contract

| Path                                | p95      | Hard timeout |
| ----------------------------------- | -------- | ------------ |
| Metric scrape (per process)         | < 100 ms | 1 s          |
| Log emit (in-process buffer push)   | < 1 ms   | 10 ms        |
| Trace export (per batch)            | < 50 ms  | 500 ms       |
| eBPF probe overhead (host-wide CPU) | < 0.5%   | 1.0% (alert) |
| Cardinality observer update         | < 100 µs | 1 ms         |
| Redaction check (per emission)      | < 50 µs  | 500 µs       |
| Registration validation             | < 5 ms   | 100 ms       |
| Backend backpressure detection      | < 1 s    | 5 s          |

These numbers are the contract surface; implementations that exceed the hard-timeout column are buggy by definition. The p95 column is the engineering target. Reference hardware is the same baseline as S2.2 / S3.1: contemporary x86_64 with NVMe SSD, 16 GB RAM, eight cores.

### §11.1 eBPF overhead is host-wide, not per-probe

The eBPF probe overhead budget (< 0.5% CPU) is the **sum** across all loaded probes, not per-probe. The probe loader (§6.4) tracks cumulative overhead and rejects further probe loads with `TELEMETRY_EBPF_PROBE_REJECTED` (FOREVER) when the budget would be exceeded.

### §11.2 Backpressure is observed, not absorbed

Telemetry backpressure (Loki unreachable, Prometheus scrape backed up, OTLP collector slow) is **observed** as a degradation, not absorbed by ever-growing queues. The in-memory ring buffer (§3.3) is bounded; beyond its bound, INFO/WARN/DEBUG drops occur with rate-limited `TELEMETRY_BACKEND_DEGRADED` evidence. The bounded-queue discipline mirrors S14.1 §6.3 (no infinite retry).

## §12 Worked examples

### §12.1 Prometheus metric — action throughput

The capability runtime (L3) emits a counter for action throughput, broken down by adapter kind and result kind.

**Registration:**

```yaml
signal_id: aios.l3.capability_runtime.action_throughput_total
kind: METRIC
metric_kind: COUNTER
budget: MEDIUM_200
retention_tier: HOT_30D
allowed_labels:
  - layer # 11 values
  - component # ≤ 50 values; here always "capability_runtime"
  - adapter_kind # closed enum from S10.1; ≈ 12 values
  - result_kind # closed enum from S14.2; 5 values
emitting_layer: L3
emitting_component: capability_runtime
description: "Counts of typed action executions terminated through the runtime, by adapter and outcome class."
redaction_profile: default
```

**Cardinality computation at registration:** 1 (this layer) × 1 (this component) × 12 (adapter kinds) × 5 (result kinds) = 60 distinct combinations. Within `MEDIUM_200` budget. Registration accepted.

**Emission, in pseudo-code:**

```rust
opentelemetry::metric::counter(
    "aios.l3.capability_runtime.action_throughput_total",
    1, // increment
    &[
        ("layer", "L3"),
        ("component", "capability_runtime"),
        ("adapter_kind", adapter_kind_enum_value()),
        ("result_kind", result_kind_enum_value()),
    ],
);
```

The adapter kind and result kind values are closed-enum strings drawn from the AIOS catalog. The action_id, subject_id, and other unbounded identifiers are deliberately absent.

**Operator query (PromQL):**

```promql
sum by (adapter_kind, result_kind) (
    rate(aios_l3_capability_runtime_action_throughput_total[5m])
)
```

This produces a small panel of bars showing action rate per adapter per result class — exactly the operational signal needed without leaking any subject or action identity.

### §12.2 Loki log — adapter event

An L3 adapter emits a structured log on adapter health-probe failure. This is a LOG signal, not an evidence record (the corresponding evidence record is `FAILURE_OBSERVED` per S14.1, emitted independently at the same site).

**Registration:**

```yaml
signal_id: aios.l3.adapter.health_probe_failed
kind: LOG
default_log_level: ERROR
budget: SMALL_50
retention_tier: HOT_30D
allowed_labels:
  - layer # 11 values
  - component # ≤ 50 values
  - failure_class # S14.1 enum; 15 values
emitting_layer: L3
emitting_component: capability_runtime
description: "Logged when an adapter health probe fails. Closed-enum-only labels; message uses sanitized template."
redaction_profile: strict
```

**Emission template (the static message):**

```text
"Adapter health probe failed: layer={layer} component={component} failure_class={failure_class}"
```

Every variable in the template is a closed-enum value resolved at emission. There are no free-form fields. The redaction layer's pattern catalog does not match.

**Operator query (LogQL):**

```logql
{layer="L3", component="capability_runtime"} |= "health probe failed"
| json
| failure_class != ""
```

Operators see: timestamp, layer, component, failure_class, message. They do not see: which adapter id failed, which subject was affected, which action_id was in flight. To trace those, the operator pivots from the log line's correlation_id to the evidence log (S3.1), where the action-bound details live.

### §12.3 eBPF trace — syscall pattern

The eBPF probe `syscall_count` runs against a closed allowlist of syscalls and emits aggregated counts. This is a METRIC signal, sourced from kernel space.

**Registration (made by the eBPF probe loader on probe install):**

```yaml
signal_id: aios.l1.kernel.syscall_count_total
kind: METRIC
metric_kind: COUNTER
budget: LARGE_1000
retention_tier: HOT_30D
allowed_labels:
  - layer # 11 values; here always "L1"
  - component # ≤ 50 values; here "kernel"
  - syscall_name # closed catalog of ≈ 50 monitored syscalls
  - result_kind # closed enum; 5 values
emitting_layer: L1
emitting_component: kernel
description: "Aggregated syscall counts from eBPF probe template syscall_count."
redaction_profile: default
```

**Cardinality computation:** 1 × 1 × 50 × 5 = 250. Within `LARGE_1000` budget.

**Probe load evidence:** `TELEMETRY_EBPF_PROBE_LOADED` (STANDARD_24M; §13) with `probe_template = "syscall_count"`, `loader_subject_id = "_system"`, `loaded_at = <timestamp>`.

**Operator query (PromQL):**

```promql
topk(10,
    sum by (syscall_name) (
        rate(aios_l1_kernel_syscall_count_total[1m])
    )
)
```

This produces the top 10 most-frequent syscalls system-wide — useful for capacity planning and anomaly detection — without exposing per-process or per-user identity.

#### §12.3.1 Why the syscall name is allowed as a label

`syscall_name` is allowed because it is drawn from a closed catalog of monitored syscalls. The eBPF probe loader rejects requests to monitor syscalls outside the catalog at probe-install time (per §6.4). The label is therefore bounded by the catalog cardinality, not by the kernel's syscall table size.

## §13 Evidence record types queued for S3.1

The following evidence record types are added to the closed `RecordType` catalog at S3.1's next consolidation pass. Ten record types in total. Each follows the S3.1 record-type discipline: one payload schema per record type, drawn from the spec's `aios.telemetry.v1alpha1` package.

| #   | Record type                         | Retention class | When emitted                                                                                                                                                                                                                                 |
| --- | ----------------------------------- | --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | `TELEMETRY_PIPELINE_STARTED`        | `STANDARD_24M`  | A registration is accepted (or rejected, with `outcome = REJECTED`). Carries `signal_id`, `kind`, `budget`, `retention_tier`, `registrar_subject_id`, `outcome`. Corresponds to §4.2 lifecycle terminal state.                               |
| 2   | `TELEMETRY_CARDINALITY_BREACH`      | `FOREVER`       | A signal exceeds its declared `CardinalityBudget` and the AUTO_DEMOTE path triggers. Carries `signal_id`, `budget`, `observed_combinations`, `demoted_label`, `breach_first_seen_at`. Per §9.3.                                              |
| 3   | `TELEMETRY_REDACTION_FAILED`        | `FOREVER`       | The redaction layer rejected an emission. Carries `signal_id`, `pattern_class`, `emitter_subject_redacted`, `dropped_at`. Per §8.5. Does not carry the offending content (that would itself be a leak).                                      |
| 4   | `TELEMETRY_BACKEND_UNAVAILABLE`     | `EXTENDED_60M`  | A backend (Prometheus, Loki, OTLP collector) became unreachable. Carries `backend`, `endpoint_redacted`, `failure_class` (S14.1), `unreachable_since_at`. Rate-limited to one per minute per backend.                                        |
| 5   | `TELEMETRY_BACKEND_DEGRADED`        | `EXTENDED_60M`  | A backend is reachable but experiencing backpressure. Carries `backend`, `degradation_observed` (closed enum: `SCRAPE_SLOW`, `INGEST_REJECTED`, `RING_BUFFER_FULL`), `since_at`. Rate-limited to one per minute per backend.                 |
| 6   | `TELEMETRY_LOG_INJECTION_DETECTED`  | `FOREVER`       | The log-line escape sanitizer rejected an emission. Carries `signal_id`, `field_name`, `sanitizer_pattern_class`. Per §10.2. Does not carry the offending content.                                                                           |
| 7   | `TELEMETRY_RETENTION_TIER_PROMOTED` | `STANDARD_24M`  | A signal's `RetentionTier` was changed (e.g. `HOT_30D → WARM_90D` for capacity reasons, or via re-registration). Carries `signal_id`, `previous_tier`, `new_tier`, `registrar_subject_id`.                                                   |
| 8   | `TELEMETRY_SAMPLING_RATE_ADJUSTED`  | `STANDARD_24M`  | A signal's sampling rate was changed (per §7.4). Carries `signal_id`, `previous_rate`, `new_rate`, `registrar_subject_id`, `reason_code`.                                                                                                    |
| 9   | `TELEMETRY_EBPF_PROBE_LOADED`       | `STANDARD_24M`  | An eBPF probe from the closed catalog (§6.4) was loaded. Carries `probe_template`, `loader_subject_id` (must be `_system`), `cumulative_overhead_observed`, `loaded_at`.                                                                     |
| 10  | `TELEMETRY_EBPF_PROBE_REJECTED`     | `FOREVER`       | An eBPF probe load was rejected (probe outside the closed catalog, overhead budget exceeded, AI subject attempting load). Carries `probe_template_requested`, `requesting_subject_id`, `rejection_reason_code` (closed enum), `rejected_at`. |

Total record types added by this spec: 10. After this addition, the S3.1 `RecordType` vocabulary grows by 10 entries (S3.1 §24 noted 87; this brings the running total to **97 entries** pending S3.1 consolidation).

### §13.1 Record-type payload package

All ten payloads live in `aios.telemetry.v1alpha1`:

```proto
syntax = "proto3";
package aios.telemetry.v1alpha1;

import "google/protobuf/timestamp.proto";

message TelemetryPipelineStartedPayload {
  string signal_id           = 1;
  TelemetrySignal kind       = 2;
  CardinalityBudget budget   = 3;
  RetentionTier retention_tier = 4;
  string registrar_subject_id = 5;
  string outcome             = 6;   // closed enum: ACCEPTED / REJECTED
  string reason_code         = 7;   // closed enum on REJECTED
}

message TelemetryCardinalityBreachPayload {
  string signal_id           = 1;
  CardinalityBudget budget   = 2;
  uint64 observed_combinations = 3;
  string demoted_label       = 4;
  google.protobuf.Timestamp breach_first_seen_at = 5;
}

message TelemetryRedactionFailedPayload {
  string signal_id           = 1;
  string pattern_class       = 2;   // closed enum
  string emitter_subject_redacted = 3;
  google.protobuf.Timestamp dropped_at = 4;
}

message TelemetryBackendUnavailablePayload {
  string backend             = 1;   // closed enum: PROMETHEUS / LOKI / OTLP_COLLECTOR / EBPF_LOADER
  string endpoint_redacted   = 2;
  aios.failure.v1alpha1.FailureClass failure_class = 3;
  google.protobuf.Timestamp unreachable_since_at = 4;
}

message TelemetryBackendDegradedPayload {
  string backend             = 1;
  string degradation_observed = 2;  // closed enum
  google.protobuf.Timestamp since_at = 3;
}

message TelemetryLogInjectionDetectedPayload {
  string signal_id           = 1;
  string field_name          = 2;
  string sanitizer_pattern_class = 3;  // closed enum
}

message TelemetryRetentionTierPromotedPayload {
  string signal_id           = 1;
  RetentionTier previous_tier = 2;
  RetentionTier new_tier     = 3;
  string registrar_subject_id = 4;
}

message TelemetrySamplingRateAdjustedPayload {
  string signal_id           = 1;
  double previous_rate       = 2;
  double new_rate            = 3;
  string registrar_subject_id = 4;
  string reason_code         = 5;
}

message TelemetryEbpfProbeLoadedPayload {
  string probe_template      = 1;
  string loader_subject_id   = 2;
  double cumulative_overhead_observed = 3;
  google.protobuf.Timestamp loaded_at = 4;
}

message TelemetryEbpfProbeRejectedPayload {
  string probe_template_requested = 1;
  string requesting_subject_id = 2;
  string rejection_reason_code = 3;  // closed enum
  google.protobuf.Timestamp rejected_at = 4;
}
```

### §13.2 Why FOREVER for breach / redaction / injection / probe-rejection

`TELEMETRY_CARDINALITY_BREACH`, `TELEMETRY_REDACTION_FAILED`, `TELEMETRY_LOG_INJECTION_DETECTED`, and `TELEMETRY_EBPF_PROBE_REJECTED` are FOREVER because they encode adversarial-signal observations: each is the canonical record of "an attempt was made to misuse the telemetry pipeline." Operators auditing the pipeline's integrity over years need these records intact; the storage cost is bounded by the rate-limit and the adversary count.

### §13.3 Why EXTENDED_60M for backend-unavailable / backend-degraded

The two backend-state events are operational rather than constitutional. Sixty months captures multi-year capacity-planning trends without bloating the FOREVER tier. The retention is the same as S14.1's similar `FAILURE_OBSERVED` rows for vault-unavailable and AI-provider-unavailable (S14.1 §4.1 rows 14, 16).

### §13.4 Why STANDARD_24M for the lifecycle records

`TELEMETRY_PIPELINE_STARTED`, `TELEMETRY_RETENTION_TIER_PROMOTED`, `TELEMETRY_SAMPLING_RATE_ADJUSTED`, and `TELEMETRY_EBPF_PROBE_LOADED` are routine lifecycle events. Twenty-four months is the standard operational evidence horizon, sufficient for the vast majority of "when was this signal first registered" or "when did sampling change" investigations.

## §14 Acceptance criteria

This sub-spec is `REAL` at `E1` when the following are all true. Promotion to higher evidence grades requires implementation milestones beyond this spec's scope.

1. The closed `TelemetrySignal` enum has exactly three values plus UNSPECIFIED, with the enumeration matching §3.1.
2. The closed `MetricKind` enum has exactly four values plus UNSPECIFIED, matching §3.2.
3. The closed `LogLevel` enum has exactly five values plus UNSPECIFIED, with `FATAL` deliberately absent, matching §3.3 and §3.3.1.
4. The closed `CardinalityBudget` enum has exactly five values plus UNSPECIFIED, with `XXL_RESERVED` documented as not-allocatable, matching §3.4.
5. The closed `RetentionTier` enum has exactly three values plus UNSPECIFIED, distinct from S3.1's `RetentionClass` per §3.5.1, matching §3.5.
6. The forbidden-as-label catalog (§5.1) lists every identifier kind cited in the registration brief, plus the enumerated additions, with rationale per row.
7. The allowed-as-label catalog (§5.2) explicitly enumerates the closed-enum-only discipline.
8. The OpenTelemetry / Prometheus / Loki / eBPF backend map (§6) names each backend's role with no overlap.
9. The sampling policy (§7) declares 1% as default for traces, 0% as default for DEBUG, and metrics as not-sampled.
10. The redaction layer (§8) lists the forbidden-content catalog and explicitly forbids the `debug_capture` profile (§8.4).
11. The cardinality-breach handling (§9) follows the AUTOMATIC_RETRY → AUTO_DEMOTE-the-label → FOREVER-evidence sequence.
12. The adversarial robustness section (§10) names at least four threats with mitigations (§10.1–§10.6).
13. The performance contract (§11) gives p95 numbers for metric scrape, log emit, trace export, and eBPF overhead matching the brief.
14. Three worked examples (§12) cover Prometheus, Loki, and eBPF respectively.
15. Ten evidence record types are queued for S3.1 with retention classes per §13 and full payload definitions in `aios.telemetry.v1alpha1`.
16. The cited invariants (INV-005, INV-014, INV-015) and the consumed sub-specs (S3.1, S2.4, S14.1, S0.1, S4.1, S8.1) are explicitly named with the relationship to this contract.

When all sixteen are satisfied (they are, by construction in this document), the spec is `REAL` at `E1`.

## §15 Cross-spec dependencies

| Spec                          | Relationship                                                                                                                                                        |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **L0 INV-005**                | Evidence is append-only; telemetry is **not** evidence and the separation is enforced here (§3.1.2).                                                                |
| **L0 INV-014**                | No proof, no completion; telemetry never substitutes for evidence at status promotion (§3.1.2).                                                                     |
| **L0 INV-015**                | Evidence never contains secrets; this contract extends the rule to telemetry (§8).                                                                                  |
| **S3.1 Evidence Log**         | The ten new record types in §13 are queued for S3.1 consolidation. Telemetry uses `RetentionTier`; evidence uses `RetentionClass`. Distinct types.                  |
| **S2.4 Verification Grammar** | The probe-error vs verification-fail distinction is mirrored at §10.2.1 between sanitizer rejection and redaction rejection.                                        |
| **S14.1 Failure Handling**    | `FailureClass`, `DegradationLevel`, and `BehaviorOnFailure` are consumed as label values (§5.2). The cardinality-breach handling mirrors §6.3.                      |
| **S0.1 Action Envelope**      | Action throughput is observed (§12.1) but `action_id` is forbidden as a label (§5.1).                                                                               |
| **S4.1 Namespace Layout**     | `ScopeKind` is allowed as a label (§5.2); `group_id` and `user_id` are forbidden (§5.1). Group/user identity stays out of the operational tier.                     |
| **S6.3 Receipt Schema**       | The TAI64N constitutional clock is shared at §10.3; trace and metric timestamps are anchored the same way evidence-receipt timestamps are anchored.                 |
| **S8.1 Network Policy**       | Telemetry exporters obey egress rules; loopback-only by default per INV-006; LAN exposure of `/metrics` requires the same `WEB_LAN_EXPOSURE_GRANTED` evidence path. |
| **S10.1 Capability Runtime**  | Adapter-kind enum is the source of `adapter_kind` label values (§5.2 row).                                                                                          |

## §16 Open deferrals

- Multi-host telemetry federation (cross-host metric / trace / log aggregation) — deferred to a future operational sub-spec; this spec is single-host.
- Operator-facing dashboard catalog (which dashboards ship by default, what panels they show) — operator documentation, not contract surface.
- Per-tenant telemetry isolation in multi-tenant deployments — deferred until the multi-tenancy story is itself specified.
- Automated anomaly detection on telemetry streams (turning telemetry into alerts) — orthogonal concern; would be a future S14.x sub-spec on alerting and runbook routing.
- Custom eBPF probe authorship — deferred indefinitely. The closed catalog is intentional (§6.4.1).
- Long-tail label demotion strategies (more sophisticated than picking the highest-cardinality contributor) — future enhancement.

## §17 See also

- [S3.1 Evidence Log Architecture](./01_evidence_log.md) — the evidence backbone, distinct from this telemetry pipeline.
- [S2.4 Verification Grammar](./02_verification_grammar.md) — the probe-error vs verification-fail discipline mirrored here.
- [S14.1 Failure Handling and Degradation](./03_failure_handling.md) — the failure taxonomy whose enums are consumed as labels here.
- [L0 Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md) — INV-005, INV-014, INV-015 cited throughout.
- [L0 Evidence Receipt Schema](../L0_Governance_Evidence_Safety/03_evidence_receipt_schema.md) — the TAI64N clock anchor (§10.3).
- [S0.1 Action Envelope + Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md) — the action surface whose throughput is observed (§12.1) but whose identifiers are not labels.
- [S4.1 AIOS-FS Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md) — `ScopeKind` is allowed as a label; `group_id` / `user_id` are forbidden.
- [S8.1 Network Policy](../L8_Network_Hardware_Devices/02_network_policy.md) — egress rules for telemetry exporters.
- [Rev.2 Master Index](../00_MASTER_INDEX.md).
