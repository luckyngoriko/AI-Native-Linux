# Latency Tiering (Rev.2)

| Field     | Value                                                                              |
| --------- | ---------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (refined 2026-05-08; awaiting implementation evidence)                  |
| Phase tag | S1.2                                                                               |
| Layer     | L5 Cognitive Core                                                                  |
| Consumes  | S1.1 Capability Translator, S0.1 Action Envelope, L0 status taxonomy, L4 policy    |
| Produces  | Typed routing decisions for direct, local-model, and external paths plus telemetry |
| Approved  | 2026-05-08 (deltas D1–D12 applied; replaces draft from `dfa3be5`)                  |

## 1. Purpose

Latency Tiering decides **when** AIOS should use deterministic code, a small local model, a powerful local model, or an external model. It prevents every user action from becoming an expensive LLM round trip while keeping safety properties intact.

The goal is not maximum intelligence on every request. The goal is the **smallest cognition path that can safely produce a typed, verifiable result**.

## 2. Core invariant

No tier may bypass typed actions, policy checks, verification, or evidence.

```text
fast path != unsafe path
slow path != more privileged path
```

The tier only decides **how cognition happens before an action draft exists**. Policy and execution are downstream of routing.

## 3. Tiers

| Tier | Name               | Typical use                                   | Model use         | Output                            |
| ---- | ------------------ | --------------------------------------------- | ----------------- | --------------------------------- |
| T0   | Cached UI/state    | open recent project, show status, recall view | none              | UI state or read-only query       |
| T1   | Deterministic      | `restart nginx`, `install docker`             | none              | one action draft from exact rules |
| T2   | Catalog retrieval  | known action, fuzzy wording                   | optional rerank   | one or more action drafts         |
| T3   | Local cognitive    | ambiguous goal, target extraction             | local model       | clarification or action drafts    |
| T4   | Powerful reasoning | multi-step goal, novel workflow               | powerful/external | plan plus action drafts           |

T0 and T1 must work without external AI. Boot, recovery, and basic administration depend on these paths.

## 4. Latency budgets

Authoritative numbers per tier. S1.1 §19 defers to this section.

### 4.1. Per-tier budgets

| Tier | p50    | p95    | p99    | Hard timeout | LLM call?          |
| ---- | ------ | ------ | ------ | ------------ | ------------------ |
| T0   | 2 ms   | 10 ms  | 25 ms  | 100 ms       | None               |
| T1   | 15 ms  | 50 ms  | 120 ms | 500 ms       | None               |
| T2   | 50 ms  | 200 ms | 400 ms | 1 s          | None (rerank only) |
| T3   | 250 ms | 800 ms | 1.5 s  | 3 s          | One local call     |
| T4   | 1 s    | 3 s    | 6 s    | 10 s         | One or more        |

p95 numbers align with S1.1 §19 (direct < 50 ms, lexical < 200 ms, RAG < 800 ms, reasoning < 3 s). If S1.1 and this section diverge on numbers, **this section wins**.

Hard timeout = wall clock from `Route()` entry to terminal response. Exceeding triggers `RoutingTimeout` (see §8.4).

### 4.2. Cold start

| Operation                                   | Budget            |
| ------------------------------------------- | ----------------- |
| Router process cold start                   | < 500 ms          |
| Catalog load (T1/T2 path readiness)         | < 2 s             |
| Local model warmup (T3 first invocation)    | < 5 s             |
| Subsequent local model invocations          | < 100 ms overhead |
| External model first call (cold connection) | < 1.5 s overhead  |

### 4.3. Backpressure

When the router is overloaded:

- T0 and T1 remain available (no LLM, no external dependency).
- T2 returns reduced candidates without rerank.
- T3 returns `NEEDS_CLARIFICATION` with `RetryAfter` rather than queueing.
- T4 is shed first; falls back to T3.

This preserves the deterministic fast path under adversarial load.

## 5. Privacy classes

A closed enum. Every `RoutingRequest` carries one. The router enforces tier restrictions per class.

| Class            | Meaning                                                                                            | Default tier ceiling           |
| ---------------- | -------------------------------------------------------------------------------------------------- | ------------------------------ |
| `PUBLIC`         | No sensitive info; e.g. published docs, public API examples.                                       | T4 (any)                       |
| `INTERNAL`       | Org/project context, but no secrets; e.g. service names, project paths.                            | T4 (any) with policy           |
| `SENSITIVE`      | Identifiable user data (names, emails, hostnames); local tiers preferred.                          | T4 with policy + approval      |
| `SECRET_BEARING` | References to secret material (vault refs, key paths, credential hints, secret-shaped substrings). | T4 local only (NEVER external) |
| `CLASSIFIED`     | Operator-marked classified context.                                                                | T2 max                         |

### 5.1. Per-class allowed tiers

| Class            | T0  | T1  | T2  | T3 local | T3 external       | T4 local | T4 external       |
| ---------------- | --- | --- | --- | -------- | ----------------- | -------- | ----------------- |
| `PUBLIC`         | ✓   | ✓   | ✓   | ✓        | ✓                 | ✓        | ✓                 |
| `INTERNAL`       | ✓   | ✓   | ✓   | ✓        | policy            | ✓        | policy            |
| `SENSITIVE`      | ✓   | ✓   | ✓   | ✓        | policy + approval | ✓        | policy + approval |
| `SECRET_BEARING` | ✓   | ✓   | ✓   | ✓        | NEVER             | ✓        | NEVER             |
| `CLASSIFIED`     | ✓   | ✓   | ✓   | NEVER    | NEVER             | NEVER    | NEVER             |

`NEVER` means the router refuses regardless of policy or approval. `policy` means subject to L4 policy decision. `policy + approval` means policy decision plus explicit human approval.

### 5.2. Class assignment

| Source                                                    | Effect                                            |
| --------------------------------------------------------- | ------------------------------------------------- |
| Default if unspecified                                    | `SENSITIVE` (conservative)                        |
| Intent Engine content analysis                            | Reduces toward `PUBLIC`/`INTERNAL` when justified |
| Detection of secret-shaped content (S1.1 §17.2.6)         | Raises to `SECRET_BEARING`                        |
| Operator policy stamp                                     | May set `CLASSIFIED`                              |
| Subject-supplied class higher than detection would assign | Honored (operator can over-classify, never under) |

The router never lowers a class. Once `SECRET_BEARING` is set, it stays.

## 6. Routing inputs

The router evaluates:

- user utterance (raw, retained for evidence; not used directly for routing decision past Intent Engine normalization)
- normalized intent and active context
- privacy class (§5)
- requested operation type (read vs state-changing)
- risk flags estimated by S1.1
- capability catalog match confidence and margin
- target schema completeness
- local model availability and warm state
- external model availability and budget remaining
- recovery mode flag
- policy egress posture (`external_egress_forbidden`)
- user routing preferences (§16)
- per-subject rate limit state
- current tier load and backpressure signals

## 7. Routing algorithm

Deterministic decision tree. Replaces the rev.1 condition table; ambiguity is removed.

### 7.1. Priority-ordered guards

```text
Guards(request) -> allowed_tiers (set), reasons (list):

  1. RecoveryMode active?
     -> allowed = {T0, T1};                               reason="recovery_mode"

  2. PrivacyClass = CLASSIFIED?
     -> allowed = {T0, T1, T2};                           reason="classified_context"

  3. PrivacyClass in {SECRET_BEARING}?
     -> drop external from allowed;                       reason="secret_bearing"

  4. Policy.external_egress_forbidden?
     -> drop external from allowed;                       reason="policy_no_egress"

  5. ExternalModelBudgetExhausted?
     -> drop external from allowed;                       reason="budget_exhausted"

  6. SubjectRateLimitTriggered?
     -> deny request entirely (REFUSED);                  reason="rate_limit_subject"
```

### 7.2. Tier selection

After guards:

```text
SelectTier(request, allowed) -> (tier, outcome):

  7. T0 cache hit (§10) AND request is read-only?
     -> tier=T0, outcome=CACHED

  8. Exact action match in catalog (per S1.1 §8.2)?
     -> tier=T1 if T1 in allowed, outcome=TRANSLATE

  9. Lexical match with margin >= 0.10 (per S1.1 §8.3)?
     -> tier=T2 if T2 in allowed, outcome=TRANSLATE

  10. Required target field missing?
      -> tier=T3 if T3 in allowed, outcome=CLARIFY

  11. Multiple high-confidence candidates (margin < 0.10)?
      -> tier=T3 if T3 in allowed, outcome=CLARIFY

  12. Multi-action operational goal (multi_action=true OR multi-verb utterance)?
      -> tier=T4 if T4 in allowed, outcome=PLAN
      -> else tier=T3, outcome=CLARIFY (decompose with user)

  13. Default
      -> tier=T3 if T3 in allowed, outcome=TRANSLATE
      -> else tier=T2, outcome=TRANSLATE (degraded)
```

### 7.3. Tie-break

When multiple steps would match, **lowest tier wins**. T1 beats T2 beats T3 beats T4. The tree is evaluated top-down; first match wins.

### 7.4. Refusal at routing level

If the selected tier is not in `allowed` AND no lower fallback is available, the router returns `REFUSED` with the appropriate guard reason. This is distinct from S1.1 translator refusal — routing refusal happens **before** translation is attempted.

## 8. Tier dynamics: upgrade, downgrade, fallback

### 8.1. Auto-upgrade

The router may upgrade a tier within a single request:

| From | To  | Trigger                                                                              |
| ---- | --- | ------------------------------------------------------------------------------------ |
| T1   | T2  | T1 found no exact match within budget                                                |
| T2   | T3  | T2 confidence below threshold AND target binding fails                               |
| T3   | T4  | Translator returned `NEEDS_CLARIFICATION` for multi-step semantics AND T4 in allowed |

**Maximum one upgrade per request.** Second upgrade attempt = fail closed with `RoutingExhausted`. Caller must retry with adjusted request (e.g. clearer utterance, MULTI_ACTION mode).

### 8.2. Auto-downgrade (degradation)

| Trigger                               | Effect                                                     |
| ------------------------------------- | ---------------------------------------------------------- |
| T4 unavailable                        | T4 → T3; `degraded=true`                                   |
| T3 model error                        | T3 → T2; `degraded=true`; record `model_error` in evidence |
| External model timeout (single retry) | Retry once on local model; if fail, downgrade tier         |
| Local model warmup not complete       | Block T3+ until warm or budget elapses                     |

### 8.3. Hard timeout per tier

On hard timeout (§4.1):

- Router does **not** silent-fallback to a lower tier.
- Returns `outcome=TIMEOUT` with the tier that timed out.
- Caller decides whether to retry with adjusted preferences.

This avoids hidden quality regressions when system is degraded.

### 8.4. Fallback evidence trail

Every fallback step appends to `RoutingEvidence.fallback_chain`:

```text
fallback_chain = [
  { from_tier: T4, to_tier: T3, reason_code: "external_unavailable" },
  { from_tier: T3, to_tier: T2, reason_code: "local_model_error" }
]
```

Renderers and audit consumers can detect chains > 1 and alert if persistent.

## 9. Direct path (T1)

Deterministic compiler from common utterance patterns to capability translator requests.

Examples:

| Input                   | Direct action                      |
| ----------------------- | ---------------------------------- |
| `restart nginx`         | `service.restart {service:nginx}`  |
| `status docker`         | `service.status {service:docker}`  |
| `install docker`        | `package.install {package:docker}` |
| `open latest project x` | read-only AIOS-FS view query       |

Direct path still emits translation evidence. It records `model_ids=[]`.

## 10. T0 caching semantics

T0 is the optional cache layer for read-only outcomes.

### 10.1. Cacheable

- Read-only catalog lookups (e.g. "what does `service.restart` do?").
- Status snapshots within their TTL (e.g. `service.status` results).
- Already-translated read-only AIOS-FS views.

### 10.2. Never cacheable

- State-changing actions (regardless of repeat input).
- Requests with `privacy_class ∈ {SECRET_BEARING, CLASSIFIED}`.
- Requests where the subject's authentication state changed since cache write.
- Requests during `recovery_mode=true`.
- Requests with `dry_run=SIMULATE` (each simulation must be fresh).

### 10.3. Cache key

```text
cache_key = "rtc_" + hex_lower(BLAKE3(JCS({
  utterance_normalized,
  privacy_class,
  catalog_version,
  subject,
  context_facts_digest
})))[:32]
```

Same encoding rules as S0.1 §8.5 / S1.1 §6.3 — lowercase hex, BLAKE3-256, 32-char truncation.

### 10.4. TTL defaults

| Cached content         | TTL  |
| ---------------------- | ---- |
| Status snapshots       | 60 s |
| Catalog explanations   | 5 m  |
| Read-only view results | 5 m  |

### 10.5. Invalidation triggers

- Catalog version flip (all entries with the old `catalog_version` invalidated).
- Evidence event indicating state change to a referenced resource.
- Subject session expiry.
- Operator-triggered cache flush.

## 11. Local model path (T3)

Used when language parsing is needed but private context should not leave the machine.

Allowed:

- target extraction
- phrasing normalization
- clarification generation
- candidate ranking from already retrieved catalog snippets

Forbidden:

- invent capabilities
- expose secrets
- decide policy
- produce shell commands for execution

## 12. Powerful reasoning path (T4)

T4 is reserved for goals that require planning, decomposition, or unfamiliar context.

T4 may produce:

- an intent object
- a plan object
- ordered calls to S1.1 translation
- clarification questions
- explanation

T4 may **not** produce executable side effects directly. Each plan step still becomes an S0.1 action envelope through S1.1.

## 13. Adversarial protection and cost model

The router is a real attack surface. Untrusted subjects can drain external API budgets, force expensive reasoning, or stress local resources.

### 13.1. Threat model

- Subject sends harmless-looking utterances at high volume to drain external API budget.
- Subject crafts utterance to force T4 reasoning unnecessarily ("explain everything about ...").
- Subject smuggles secret-bearing content past the privacy classifier.
- Subject uses approved access to harvest expensive answers (cost exfiltration).

### 13.2. Rate limits

Default per-subject rate limits (configurable):

| Scope                   | Default limit |
| ----------------------- | ------------- |
| Routing requests/minute | 60            |
| T3 invocations/minute   | 20            |
| T4 invocations/hour     | 10            |
| External calls/day      | 200           |

Implementation: token bucket with burst capacity of 1.5× refill rate.

### 13.3. External model budget

Operator sets daily and/or monthly budgets in either token count or USD-equivalent:

| Threshold      | Effect                                        |
| -------------- | --------------------------------------------- |
| 80 % consumed  | Alert via L9 telemetry                        |
| 95 % consumed  | T4-external → T4-local forced                 |
| 100 % consumed | T4 → T3 forced; T3-external → T3-local forced |

Budget consumption tracked per `(subject, model_id)` and aggregated per organization.

### 13.4. Defense delegation

| Concern                             | Owner                              |
| ----------------------------------- | ---------------------------------- |
| Subject authentication              | L4                                 |
| Privacy classification of utterance | Intent Engine + S1.1 §17 detection |
| Routing-level rate enforcement      | This spec                          |
| External API key management         | L4 Vault Broker                    |
| Budget accounting persistence       | L9                                 |
| Operator alerts                     | L9 telemetry                       |

## 14. Degraded mode

AIOS must continue useful operation when high tiers are unavailable.

| Failure                    | Required degradation                                             |
| -------------------------- | ---------------------------------------------------------------- |
| External model unavailable | Use local tiers; mark `degraded=true`                            |
| Local model unavailable    | Use deterministic paths (T0/T1/T2); require exact typed commands |
| Vector index unavailable   | Lexical/exact catalog search only                                |
| Catalog unavailable        | Block state-changing translation; allow T0 cache reads only      |
| External budget exhausted  | T4 → T3; T3-external → T3-local                                  |
| Recovery mode active       | T0/T1 only                                                       |
| Subject rate limited       | Refuse with reason; surface retry-after                          |

Degraded routing is evidence-worthy. The user-facing renderer may show reduced cognition, but action safety is unchanged.

## 15. Statelessness contract

The router is **stateless across `Route()` calls** (same discipline as S1.1 §18).

### 15.1. What this means

- No accumulation of subject context across routing calls.
- No memory of previous routing decisions.
- No cache that affects routing **decision** (the T0 cache from §10 is a result cache, not a decision cache).

### 15.2. Reproducibility input set

Given:

1. The `RoutingRequest` contents.
2. The system snapshot at receipt time: `(catalog_version, recovery_mode, available_local_models, available_external_models, budget_remaining, subject_rate_limit_state)`.
3. Router code version.

The same three inputs yield the same `RoutingDecision`. This is a determinism guarantee.

### 15.3. Allowed local optimizations

- Catalog index (rebuilt on version flip).
- Embedding cache for candidate retrieval.
- Token bucket state for rate limiting (process-local; converges across instances via L9 evidence).

These are observably indistinguishable from re-computing.

## 16. User preferences

Optional hints in `RoutingRequest.preferences`. **Hints, not enforcement.**

| Field            | Effect                                                      |
| ---------------- | ----------------------------------------------------------- |
| `prefer_local`   | Strongly prefer local tiers even if external is allowed     |
| `prefer_speed`   | Accept lower-quality result for faster response (cap at T3) |
| `prefer_quality` | Accept higher latency for higher tier (raise floor to T3)   |

### 16.1. Override rules

Policy and guards always win over preferences:

```text
final_tier = max_restriction(
  preferences,        // hints
  privacy_class_ceiling,   // §5
  guards,             // §7.1
  budget_state,       // §13.3
  rate_limits,        // §13.2
  recovery_mode       // §7.1
)
```

`prefer_quality=true` cannot escalate above what privacy_class allows. `prefer_speed=true` cannot drop below what semantic clarity requires (e.g. ambiguous → T3 still required for clarification).

### 16.2. Storage

Preferences are passed per-call. The router does not persist them. Long-lived preference storage is the renderer's or session manager's concern.

## 17. Telemetry contract

The router exports the following metrics (Prometheus-style names; OpenTelemetry-compatible).

### 17.1. Required metrics

| Metric                                                    | Type      | Labels                           |
| --------------------------------------------------------- | --------- | -------------------------------- |
| `routing_decisions_total`                                 | counter   | `tier`, `outcome`, `degraded`    |
| `routing_latency_seconds`                                 | histogram | `tier`, `outcome`                |
| `routing_fallback_total`                                  | counter   | `from_tier`, `to_tier`, `reason` |
| `routing_privacy_class_total`                             | counter   | `class`                          |
| `external_model_budget_consumed_units_total`              | counter   | `model_id`                       |
| `external_model_budget_remaining_units`                   | gauge     | `model_id`                       |
| `routing_concurrent_requests`                             | gauge     | `tier`                           |
| `routing_cache_hits_total` / `routing_cache_misses_total` | counter   | `cache_class`                    |
| `routing_rate_limit_total`                                | counter   | `subject_class`, `scope`         |
| `routing_refused_total`                                   | counter   | `reason_code`                    |

### 17.2. Cardinality bounds

| Label                 | Max distinct values                                                                 |
| --------------------- | ----------------------------------------------------------------------------------- |
| `tier`                | 5 (T0–T4)                                                                           |
| `outcome`             | 7 (TRANSLATE, CLARIFY, PLAN, CACHED, REFUSED, TIMEOUT, ROUTING_OUTCOME_UNSPECIFIED) |
| `from_tier`/`to_tier` | 25 combinations                                                                     |
| `class`               | 6 (incl. UNSPECIFIED)                                                               |
| `model_id`            | bounded by available models (typically < 20)                                        |
| `reason`              | bounded vocabulary (< 30 documented codes)                                          |

**Subject is never a metric label** (high cardinality). Per-subject accounting is in evidence, not metrics.

### 17.3. Histogram buckets

Latency histograms use exponential buckets from 1 ms to 30 s: `[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30]` seconds.

## 18. Evidence chain

Routing decisions sit at the head of a four-level chain:

```text
routing_id (rt_<ULID>)
   │ produces
   ▼
translation_id (trn_<ULID>)
   │ produces (one or more)
   ▼
action_id (act_<ULID>)
   │ produces (multiple receipts)
   ▼
evidence_receipt_ids (evr_<ULID>)
```

### 18.1. Bidirectional references

| Record                | References                                                                             |
| --------------------- | -------------------------------------------------------------------------------------- |
| `RoutingEvidence`     | `translation_ids[]` (filled when downstream translations produced)                     |
| `TranslationEvidence` | `routing_id` (the routing decision that called the translator) — extension to S1.1 §15 |
| `ActionEnvelope`      | `identity.correlation_id` flows from routing → translation → action                    |
| `evidence_receipt`    | back-references to `routing_id`, `translation_id`, `action_id` (per L9 S3.1)           |

### 18.2. Fields recorded

Every routing decision evidence record contains:

- `routing_id`
- timestamp
- `subject` (canonicalized per L4)
- `privacy_class`
- `selected_tier`
- `fallback_chain[]`
- `outcome`
- `reason_code` and `reason_message`
- `model_ids_used[]`
- `budget_units_consumed`
- `catalog_version`
- linked `intent_id`, `plan_id`, `correlation_id`
- linked `translation_ids[]` (filled later)

Privacy rules from S1.1 §15 apply: prompt bodies stored only with explicit policy enable; raw secrets never present.

## 19. Routing service surface

```proto
syntax = "proto3";
package aios.cognition.routing.v1alpha1;

import "google/protobuf/empty.proto";
import "google/protobuf/struct.proto";

service LatencyRouter {
  rpc Route(RoutingRequest) returns (RoutingResponse);
  rpc GetRouterInfo(google.protobuf.Empty) returns (RouterInfo);
}
```

`Route()` is the entry point. `GetRouterInfo()` exposes router state for negotiation (similar to S0.1 `GetCapabilityRuntimeInfo`). Full IDL in **Appendix A**.

### 19.1. `Route()` semantics

- Caller supplies `RoutingRequest` with normalized intent, context, privacy class, preferences.
- Router decides tier and outcome per §7.
- Response contains `RoutingDecision` and `RoutingEvidence`.
- For `outcome=CACHED`, the cached answer is returned directly (no downstream translation).
- For other outcomes, caller proceeds to S1.1 translator with the routing decision in tow.

### 19.2. Error model

| gRPC status           | When                                                 |
| --------------------- | ---------------------------------------------------- |
| `INVALID_ARGUMENT`    | Schema violation, missing required fields            |
| `RESOURCE_EXHAUSTED`  | Subject rate limit hit                               |
| `FAILED_PRECONDITION` | Schema version unsupported                           |
| `UNAVAILABLE`         | Router degraded; caller should retry                 |
| `OK`                  | Routing decision returned (`outcome` may be REFUSED) |

## 20. Golden routing fixtures

`{ input, expected, status }` triples for an acceptance harness.

### 20.1. Exact known command (T1)

```yaml
fixture_id: rt.fix.exact_restart.v1
input:
  utterance: "restart nginx"
  privacy_class: PUBLIC
  recovery_mode: false
expected:
  tier: T1
  outcome: TRANSLATE
  fallback_chain: []
  degraded: false
```

### 20.2. Fuzzy phrasing (T2)

```yaml
fixture_id: rt.fix.fuzzy_bounce_nginx.v1
input:
  utterance: "bounce the nginx daemon"
  privacy_class: PUBLIC
expected:
  tier: T2
  outcome: TRANSLATE
```

### 20.3. Ambiguous (T3 clarify)

```yaml
fixture_id: rt.fix.ambiguous_online.v1
input:
  utterance: "put it online"
expected:
  tier: T3
  outcome: CLARIFY
```

### 20.4. Multi-step plan (T4)

```yaml
fixture_id: rt.fix.multi_step_rust.v1
input:
  utterance: "prepare a Rust dev environment"
expected:
  tier: T4
  outcome: PLAN
```

### 20.5. Recovery mode caps tier

```yaml
fixture_id: rt.fix.recovery_install_blocked.v1
input:
  utterance: "install docker"
  recovery_mode: true
expected:
  tier: T1
  outcome: TRANSLATE
note: "exact-match T1 still allowed in recovery mode"
```

```yaml
fixture_id: rt.fix.recovery_fuzzy_refused.v1
input:
  utterance: "set up docker for me"
  recovery_mode: true
expected:
  tier: T1
  outcome: REFUSED
  reason_code: "recovery_mode"
note: "fuzzy phrasing requires T2+ which is not allowed in recovery"
```

### 20.6. Secret-bearing routes local-only

```yaml
fixture_id: rt.fix.secret_bearing_local.v1
input:
  utterance: "rotate my github token"
  privacy_class: SECRET_BEARING
expected:
  tier: T3
  outcome: TRANSLATE
  selected_model_class: local
  forbidden_models: external
```

### 20.7. External budget exhausted

```yaml
fixture_id: rt.fix.budget_exhausted_fallback.v1
input:
  utterance: "explain Linux scheduling at depth"
  external_budget_remaining: 0
expected:
  tier: T3
  outcome: TRANSLATE
  fallback_chain:
    - { from_tier: T4, to_tier: T3, reason: "budget_exhausted" }
  degraded: true
```

### 20.8. Bulgarian utterance equivalence

```yaml
fixture_id: rt.fix.bg_exact.v1
input:
  utterance: "рестартирай nginx"
  privacy_class: PUBLIC
expected:
  tier: T1
  outcome: TRANSLATE
note: "Intent Engine normalizes; T1 exact-action lookup matches via aliases_localized"
```

### 20.9. Adversarial budget drain refused

```yaml
fixture_id: rt.fix.adversarial_rate_limit.v1
input:
  scenario: "subject sends 200 routing requests in 60s"
  utterance: "explain something"
expected_after_60_requests:
  tier: TIER_UNSPECIFIED
  outcome: REFUSED
  reason_code: "rate_limit_subject"
```

### 20.10. Classified context tier ceiling

```yaml
fixture_id: rt.fix.classified_t2_ceiling.v1
input:
  utterance: "summarize this document"
  privacy_class: CLASSIFIED
expected:
  tier: T2
  outcome: TRANSLATE
note: "T3+ blocked by privacy class ceiling"
```

```yaml
fixture_id: rt.fix.classified_ambiguous_refused.v1
input:
  utterance: "do something useful with this"
  privacy_class: CLASSIFIED
expected:
  tier: TIER_UNSPECIFIED
  outcome: REFUSED
  reason_code: "classified_context"
note: "Ambiguous request would need T3 clarification, but T3+ blocked by class"
```

## 21. Acceptance criteria

- Exact low-risk commands route without an LLM (T1; fixtures 20.1, 20.5).
- Ambiguous commands route to clarification (T3; fixture 20.3).
- External AI can be disabled without breaking service/package/status operations (fixtures 20.5, 20.7).
- `SECRET_BEARING` and `CLASSIFIED` contexts never route to external models (fixtures 20.6, 20.10).
- Recovery mode blocks high cognition tiers (fixture 20.5).
- Every routing decision emits structured `RoutingEvidence`.
- All golden fixtures from §20 pass against the implementation.
- Budget exhaustion triggers documented fallback chain (fixture 20.7).
- Subject rate limits trigger `REFUSED` with documented reason (fixture 20.9).
- Hard timeouts return `outcome=TIMEOUT`, never silent fallback.
- Per-tier latency p95 meets §4.1 budgets in load tests of typical throughput.
- Telemetry metrics from §17 are emitted with bounded label cardinality.
- Same `(RoutingRequest, system_snapshot, code_version)` produces the same decision (statelessness).

## 22. Cross-spec dependencies

| Spec                           | What this spec consumes / aligns with                                                                                                                                    |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **S0.1** Action Envelope       | `correlation_id` flows from routing → translation → action. Hash encoding (`hex_lower(BLAKE3(...))[:32]`) shared.                                                        |
| **S1.1** Capability Translator | Routing decisions invoke the translator. `routing_id` is referenced by `TranslationEvidence` (extension to S1.1 §15). Threshold values (§8.3 there) reused in §7.2 here. |
| **S1.3** AIOS-FS Object Model  | T0 cache invalidation uses AIOS-FS evidence-event signals.                                                                                                               |
| **S2.3** Policy Kernel         | Policy decisions on `external_egress_forbidden`, approval requirements for `SENSITIVE` external use.                                                                     |
| **S3.1** Evidence Log          | `RoutingEvidence` records land in the evidence log; `routing_id` is a queryable index.                                                                                   |
| **L4 Vault Broker**            | External model API keys live in Vault; router never reads raw keys.                                                                                                      |
| **L9 Telemetry Pipeline**      | Metrics from §17 export through the L9 telemetry pipeline.                                                                                                               |

## 23. Open deferrals

- Per-subject routing preference persistence belongs to renderer/session layer (L7).
- Cross-subject anomaly detection on routing patterns belongs to L9 anomaly detection (future).
- Distributed router consensus (multi-node routers sharing budget state) is implementation detail for HA deployments.
- Cost model in non-token units (e.g. GPU-seconds for local inference) is deferred until a measurable surface exists.
- Adaptive threshold tuning (auto-adjust §7.2 margins based on outcome quality) is deferred.

## 24. See also

- [S0.1 Action Envelope + Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S1.1 Capability Translator](02_capability_translator.md)
- [L5 Cognitive Core overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

## Appendix A: Complete proto IDL

```proto
syntax = "proto3";
package aios.cognition.routing.v1alpha1;

import "google/protobuf/empty.proto";
import "google/protobuf/struct.proto";
import "google/protobuf/timestamp.proto";

// ─────────────────────────────────────────────────────────────────
// Routing request
// ─────────────────────────────────────────────────────────────────

message RoutingRequest {
  string schema_version = 1;          // "aios.cognition.routing.v1alpha1"
  string routing_id = 2;              // optional; "rt_<ULID>"; router assigns if empty
  string intent_id = 3;
  string plan_id = 4;
  string plan_step_id = 5;
  string correlation_id = 6;
  string subject = 7;                 // provisional L4 subject string

  string utterance = 8;               // raw, retained for evidence
  string utterance_normalized = 9;    // upstream-normalized form

  PrivacyClass privacy_class = 10;
  RoutingPreference preferences = 11;
  RoutingConstraints constraints = 12;

  google.protobuf.Struct context_facts = 13;
  string context_facts_digest = 14;   // hex_lower(BLAKE3(JCS(context_facts)))[:32]
}

enum PrivacyClass {
  PRIVACY_CLASS_UNSPECIFIED = 0;
  PUBLIC          = 1;
  INTERNAL        = 2;
  SENSITIVE       = 3;
  SECRET_BEARING  = 4;
  CLASSIFIED      = 5;
}

message RoutingPreference {
  bool prefer_local = 1;
  bool prefer_speed = 2;
  bool prefer_quality = 3;
}

message RoutingConstraints {
  bool recovery_mode = 1;
  bool external_egress_forbidden = 2;
  string preferred_local_model_id = 3;
}

// ─────────────────────────────────────────────────────────────────
// Routing response
// ─────────────────────────────────────────────────────────────────

message RoutingResponse {
  string routing_id = 1;
  RoutingDecision decision = 2;
  RoutingEvidence evidence = 3;
  CachedResult cached = 4;            // populated when decision.outcome = CACHED
}

message RoutingDecision {
  Tier tier = 1;
  RoutingOutcome outcome = 2;
  string reason_code = 3;
  string reason_message = 4;
  repeated FallbackStep fallback_chain = 5;
  string selected_model_id = 6;
  bool degraded = 7;
}

enum Tier {
  TIER_UNSPECIFIED = 0;
  T0 = 1;
  T1 = 2;
  T2 = 3;
  T3 = 4;
  T4 = 5;
}

enum RoutingOutcome {
  ROUTING_OUTCOME_UNSPECIFIED = 0;
  TRANSLATE = 1;       // proceed to S1.1 translator
  CLARIFY   = 2;       // Intent Engine asks user
  PLAN      = 3;       // Planner creates multi-step plan first
  CACHED    = 4;       // T0 cache hit; no translation needed
  REFUSED   = 5;       // routing-level refusal
  TIMEOUT   = 6;       // tier hit hard timeout
}

message FallbackStep {
  Tier from_tier = 1;
  Tier to_tier = 2;
  string reason_code = 3;
}

message CachedResult {
  string cache_key = 1;
  google.protobuf.Struct payload = 2;
  google.protobuf.Timestamp cached_at = 3;
  google.protobuf.Timestamp expires_at = 4;
}

// ─────────────────────────────────────────────────────────────────
// Routing evidence
// ─────────────────────────────────────────────────────────────────

message RoutingEvidence {
  string routing_id = 1;
  google.protobuf.Timestamp occurred_at = 2;
  string subject = 3;
  PrivacyClass privacy_class = 4;
  Tier selected_tier = 5;
  RoutingOutcome outcome = 6;
  string reason_code = 7;
  repeated FallbackStep fallback_chain = 8;
  repeated string model_ids_used = 9;
  uint64 budget_units_consumed = 10;
  string catalog_version = 11;
  string intent_id = 12;
  string plan_id = 13;
  string correlation_id = 14;
  repeated string translation_ids = 15;     // filled when downstream translations produced
}

// ─────────────────────────────────────────────────────────────────
// Router info
// ─────────────────────────────────────────────────────────────────

message RouterInfo {
  string router_id = 1;
  repeated string supported_schema_versions = 2;
  string default_schema_version = 3;
  string router_version = 4;
  repeated Tier active_tiers = 5;
  repeated string available_local_model_ids = 6;
  repeated string available_external_model_ids = 7;
  bool recovery_mode_active = 8;
  uint64 external_budget_remaining_units = 9;
  google.protobuf.Timestamp started_at = 10;
}

service LatencyRouter {
  rpc Route(RoutingRequest) returns (RoutingResponse);
  rpc GetRouterInfo(google.protobuf.Empty) returns (RouterInfo);
}
```
