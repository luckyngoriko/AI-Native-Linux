# Policy Kernel (Rev.2)

| Field     | Value                                                                                  |
| --------- | -------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (refined 2026-05-08; awaiting implementation evidence)                      |
| Phase tag | S2.3                                                                                   |
| Layer     | L4 Policy, Identity, Vault                                                             |
| Consumes  | S0.1 Action Envelope, S1.3 object metadata, L3 adapter manifests, L4 identity bindings |
| Produces  | policy decisions, approval requirements, denials; gRPC `PolicyKernel`                  |
| Approved  | 2026-05-08 (deltas D1–D12 applied; replaces draft from `dfa3be5`)                      |

## 1. Purpose

The Policy Kernel is the operating constitution of AIOS. It decides whether a typed action may proceed, requires approval, or must be denied. It evaluates **typed action envelopes** (S0.1), not shell commands.

This sub-spec defines the decision pipeline, conditions and constraints vocabularies, policy bundle format and distribution, determinism and caching, performance budgets, emergency override boundary, AI self-approval prevention, adversarial robustness, the gRPC surface, and acceptance fixtures. Approval delivery, identity model details, vault mechanics, and emergency override mechanics live in companion sub-specs (`02_…`, `03_…`, `04_…`, `05_…`).

## 2. Position in the system

```text
Capability Translator (S1.1)
        |
        v
ActionEnvelope (S0.1)
        |
        v
Capability Runtime (L3)
        |
        v
PolicyKernel.EvaluatePolicy ── this spec ──▶ PolicyDecision
        |
        v
adapter execution / approval flow
        |
        v
Verification (S2.4) → Evidence (S3.1)
```

The Policy Kernel is on the hot path of every state-changing action. Its decisions are evidence-linked and bound to the exact request hash.

## 3. Decision pipeline

```text
EvaluatePolicy(envelope) -> PolicyDecision:
  1. validate envelope schema (S0.1)
  2. normalize subject       (§7)
  3. enrich resources        (§8)
  4. compute request_hash    (S0.1 §8.5)
  5. evaluate hard denies    (§6)
  6. evaluate emergency-override denylist (§16)
  7. evaluate scoped denies  (§5 step 4)
  8. evaluate scoped allows  (§5 step 5)
  9. apply AI self-approval prevention (§17)
  10. apply default deny     (§5 step 6)
  11. bind constraints       (§10)
  12. emit decision + evidence
```

Each step either short-circuits or passes the (envelope, enrichment, partial decision) to the next step. No silent fall-through is allowed; every envelope produces a decision.

## 4. Decision result

```proto
message PolicyDecision {
  string policy_decision_id = 1;                       // "poldec_<ULID>"
  string action_id = 2;                                // referencing ActionEnvelope.identity.action_id
  string request_hash = 3;                             // hex_lower(BLAKE3(canonical(request)))[:32]
  string bundle_version = 4;                           // bundle that produced the decision
  string enrichment_snapshot_id = 5;                   // for determinism (§13)
  Decision decision = 6;
  string reason_code = 7;                              // canonical short code; e.g. "ScopedAllow"
  string reason_message = 8;                           // English human-readable
  Constraints constraints = 9;
  ApprovalRequirement approval = 10;
  string evidence_receipt_id = 11;
  google.protobuf.Timestamp evaluated_at = 12;
  uint32 rules_consulted = 13;                         // for §19 budget audit
  bool simulated = 14;                                 // true if produced by SimulatePolicy
}

enum Decision {
  DECISION_UNSPECIFIED   = 0;
  ALLOW                  = 1;
  REQUIRE_APPROVAL       = 2;
  DENY                   = 3;
}
```

`request_hash` follows S0.1 §8.5 truncation rules. Approvals bind to the exact hash; if the request changes, the approval is invalid.

## 5. Rule precedence (fixed)

```text
1. Invalid subject ............................. -> DENY
2. Hard deny (§6) .............................. -> DENY
3. Emergency override denylist (§16) ........... -> DENY
4. Explicit scoped DENY rule ................... -> DENY
5. Explicit scoped ALLOW rule .................. -> ALLOW or REQUIRE_APPROVAL
6. AI self-approval prevention (§17) ........... -> may upgrade ALLOW to REQUIRE_APPROVAL
7. Default ..................................... -> DENY
```

Default deny is mandatory. Step 6 is a post-hoc filter applied after step 5 produced an ALLOW.

## 6. Hard denies

The hard-deny list is part of L0 (constitutional truth) and embedded in this spec for clarity. Hard denies cannot be overridden except as listed.

| `policy_id`                               | Class                                                                     | Override path                                                  |
| ----------------------------------------- | ------------------------------------------------------------------------- | -------------------------------------------------------------- |
| `hd.secret_raw_read_by_ai`                | Raw secret read by `agent`/`application` subject                          | None                                                           |
| `hd.recursive_delete_root`                | Recursive deletion of `/`, `/home`, `/root`, `/aios`, recovery partitions | None                                                           |
| `hd.policy_log_mutation`                  | Mutation or deletion of policy log                                        | None                                                           |
| `hd.evidence_log_mutation`                | Mutation of evidence log (§S3.1 invariant)                                | None                                                           |
| `hd.disable_policy_kernel`                | Disabling Policy Kernel (self-disable)                                    | None                                                           |
| `hd.disable_recovery_path`                | Disabling recovery path                                                   | None                                                           |
| `hd.modify_boot_chain`                    | Modifying boot chain without dedicated recovery approval                  | Recovery-mode operator approval per `05_emergency_override.md` |
| `hd.untyped_shell_privileged`             | Untyped shell execution as privileged subject                             | None                                                           |
| `hd.aios_fs_pointer_rollback_on_recovery` | Rolling back recovery-essential pointers without operator approval        | Recovery-mode operator approval                                |
| `hd.privacy_class_downgrade`              | Lowering an object's privacy class                                        | None (S1.3 §4.1)                                               |

Emergency override **cannot bypass** evidence logging. Even an authorized override emits evidence with the override receipt.

## 7. Subject normalization

The Policy Kernel accepts the provisional `<type>:<name>[/<sub_id>]` subject string from S0.1 and canonicalizes it through L4 identity:

```text
provisional      "agent:dev"
       |
       v
canonical        "agent:dev:01HX..."        (with stable canonical_subject_id)
       |
       v
hydrated subject
  - canonical_subject_id
  - subject_type (human/agent/application/service/device/workflow/remote_operator)
  - groups            (e.g. ["maintainers", "operators"])
  - capabilities      (from L4 vault grants)
  - session_class     (highest privacy ceiling subject is operating under)
  - recovery_mode     (true when operating under recovery-mode credential)
  - is_ai             (subject_type ∈ {agent, application})
```

Subject hydration is performed via the L4 identity service. If hydration fails (subject unknown, expired, revoked), the decision short-circuits to `DENY` with `reason_code = SubjectUnauthenticated`.

The hydrated subject is part of the **enrichment snapshot** (§8) and contributes to determinism (§13).

## 8. Resource enrichment

Before evaluation, the Policy Kernel reads metadata about objects referenced in the request:

| Resource                              | Source                                | Fields read                                                             |
| ------------------------------------- | ------------------------------------- | ----------------------------------------------------------------------- |
| Object referenced in `request.target` | AIOS-FS (S1.3 `ReadObject`, SNAPSHOT) | `privacy_class`, `policy_tags`, `created_by`, `lifecycle_state`, `kind` |
| Action's adapter family               | L3 adapter manifest                   | declared `risk_template`, `default_sandbox_profile_id`                  |
| Sandbox profile                       | L6 sandbox composition                | profile constraints                                                     |
| Verification grammar                  | L9 S2.4                               | required verification primitives                                        |

Enrichment uses **SNAPSHOT** consistency (S1.3 §11) to ensure all reads come from a coherent point in time. The snapshot is identified by `enrichment_snapshot_id` and recorded in the decision for determinism (§13).

If an enrichment read fails (object missing, AIOS-FS degraded), the decision short-circuits to `DENY` with `reason_code = EnrichmentUnavailable`.

## 9. Conditions vocabulary

Conditions reference enriched fields. The vocabulary is **closed**.

### 9.1. EBNF

```ebnf
condition  = predicate ( "and" predicate )* ;
predicate  = field op value
           | field "in"       "[" value ( "," value )* "]"
           | field "contains" string_literal
           | field "exists"
           | "time" "." "recovery_mode"        ;          // boolean predicate
field      = namespace "." identifier ( "." identifier )* ;
namespace  = "subject" | "request" | "target" | "object" | "time" | "system" ;
op         = "=" | "!=" | "<" | "<=" | ">" | ">=" ;
value      = string_literal | number_literal | boolean_literal | timestamp_literal | identifier_literal ;
```

Same restrictions as S2.1 query DSL: `and` only (no `or`), no parentheses, closed namespaces.

### 9.2. Namespace contents

| Namespace | Allowed fields                                                                                                                                                                |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `subject` | `canonical_subject_id`, `subject_type`, `groups`, `capabilities`, `session_class`, `recovery_mode`, `is_ai`                                                                   |
| `request` | `action`, `environment`, `risk.destructive`, `risk.privileged`, `risk.network_exposure`, `risk.secret_access`, `risk.recovery_path_affected`, `dry_run`, `sandbox_profile_id` |
| `target`  | adapter-declared target fields (e.g. `target.service`, `target.package`, `target.url`) — schema-validated by adapter                                                          |
| `object`  | `privacy_class`, `policy_tags`, `kind`, `lifecycle_state`, `created_by`                                                                                                       |
| `time`    | `recovery_mode`, `weekday`, `hour_utc`                                                                                                                                        |
| `system`  | `host_id`, `cluster_id`, `release_channel`                                                                                                                                    |

Fields outside the vocabulary cause bundle-load failure with `InvalidPolicyBundle`.

## 10. Constraints vocabulary

Constraints attach to ALLOW or REQUIRE_APPROVAL decisions and bind execution. Closed set.

| Constraint                   | Type   | Semantics                                                                                        |
| ---------------------------- | ------ | ------------------------------------------------------------------------------------------------ |
| `sandbox_profile_id`         | string | Required sandbox profile (max-restriction with caller's per S0.1 §9.2)                           |
| `max_runtime_seconds`        | uint   | Hard wall-clock cap on adapter execution                                                         |
| `verification_required`      | bool   | Require non-empty verification intents (S0.1 §3) regardless of caller                            |
| `dry_run_only`               | bool   | Decision only valid for `dry_run ∈ {VALIDATE, SIMULATE}`; LIVE invalid                           |
| `require_evidence_grade`     | enum   | Minimum evidence grade required before action terminal phase                                     |
| `require_human_co_signer`    | bool   | Approval requires a second human subject                                                         |
| `network_policy`             | enum   | `LOCALHOST_ONLY` / `LAN_ALLOWED` / `INTERNET_ALLOWED`; max-restriction with action's environment |
| `max_concurrent_per_subject` | uint   | Concurrency cap                                                                                  |
| `min_subject_session_class`  | enum   | Subject's session must be at this class or below (`PUBLIC`/`INTERNAL`/...)                       |
| `vault_capability_required`  | string | Subject must hold this Vault capability                                                          |
| `ttl_seconds`                | uint   | Decision validity TTL (default 300 s, max 3600 s)                                                |

```proto
message Constraints {
  string sandbox_profile_id = 1;
  uint32 max_runtime_seconds = 2;
  bool verification_required = 3;
  bool dry_run_only = 4;
  string require_evidence_grade = 5;          // "E2" .. "E5"
  bool require_human_co_signer = 6;
  string network_policy = 7;                   // enum string
  uint32 max_concurrent_per_subject = 8;
  string min_subject_session_class = 9;
  string vault_capability_required = 10;
  uint32 ttl_seconds = 11;
}
```

Unknown constraints in a bundle cause `InvalidPolicyBundle` at load time.

## 11. Policy rule shape

Rules are authored in YAML; the canonical proto representation is below.

### 11.1. YAML

```yaml
rule_id: allow_restart_user_services
effect: allow
priority: 100 # higher number = evaluated earlier within same step
subjects:
  - human:lucky
  - group:operators
actions:
  - service.restart
  - service.reload
conditions:
  - environment = "LOCAL"
  - target.service in ["nginx", "postgresql", "docker"]
  - object.privacy_class <= "INTERNAL"
  - subject.recovery_mode = false
constraints:
  sandbox_profile_id: host-service-control
  max_runtime_seconds: 30
  verification_required: true
approval:
  required: false
metadata:
  description: User can restart their own service set
  authors: ["luckyngoriko"]
  policy_pack: "user-base.v1"
```

### 11.2. Proto

```proto
message PolicyRule {
  string rule_id = 1;
  RuleEffect effect = 2;
  int32 priority = 3;
  repeated string subjects = 4;          // matches canonical subject or "group:..."
  repeated string actions = 5;
  repeated string conditions = 6;        // each is a single predicate string per §9.1
  Constraints constraints = 7;
  ApprovalRequirement approval = 8;
  RuleMetadata metadata = 9;
}

enum RuleEffect {
  RULE_EFFECT_UNSPECIFIED = 0;
  ALLOW_EFFECT = 1;
  DENY_EFFECT  = 2;
}

message ApprovalRequirement {
  bool required = 1;
  string approval_scope = 2;             // "exact_request_hash" (only value supported in rev.2)
  uint32 ttl_seconds = 3;                // approval validity window
  repeated string approver_classes = 4;  // subject_type filter, e.g. ["human"]
  bool require_human_co_signer = 5;
}

message RuleMetadata {
  string description = 1;
  repeated string authors = 2;
  string policy_pack = 3;
  google.protobuf.Timestamp created_at = 4;
  google.protobuf.Timestamp last_modified_at = 5;
}
```

Within a precedence step, rules are evaluated by `priority` descending, then `rule_id` lexicographic.

## 12. Policy bundle format and distribution

Mirrors the S1.1 §6.4 catalog distribution pattern.

### 12.1. Bundle structure

```text
policy_bundle/
  manifest.json                          # bundle metadata, included rules, schema version
  rules/                                 # one file per rule
    allow_restart_user_services.yaml
    deny_recursive_root.yaml
    ...
  hard_denies.yaml                       # mirror of §6 (must match)
  signatures/
    publisher.sig                        # publisher signature over canonical bundle hash
    aios_root.sig                        # AIOS root signature endorsing the publisher
```

### 12.2. Bundle identity

Content-addressed:

```text
bundle_version = "polb_" + hex_lower(BLAKE3(canonical_bundle_bytes))[:32]
```

Same encoding rules as S0.1 §8.5.

### 12.3. Trust chain

```text
AIOS root key  ──signs──▶  Publisher key  ──signs──▶  Policy bundle
```

Verification rules:

1. Bundle signature must verify against the publisher key in the AIOS trust store.
2. Publisher must be endorsed for the policy domain (e.g., `service.*` policies require service-domain endorsement).
3. Endorsement revocation honored on next bundle reload; in-flight evaluations finish on the previously trusted version.
4. Bundle signature failure → engine enters degraded mode (only L0 hard denies + emergency override path active; all other actions DENY).

### 12.4. Hot reload semantics

When a new bundle version is staged:

- Engine validates the bundle (rules parse, conditions reference allowed namespaces, constraints valid, no rule cycles per §19).
- New evaluations receive the new bundle atomically when validation completes.
- In-flight evaluations finish on the version they started with.
- Old version retained for `evidence_grace_period` (default 1 hour) to support audit queries referencing prior decisions.

### 12.5. Operator-only rollback

Operators may force rollback to a previous known-good `bundle_version` via an explicit, evidence-logged operation. The engine never rolls back autonomously.

## 13. Decision determinism and caching

### 13.1. Determinism

Given the triple `(request_hash, bundle_version, enrichment_snapshot_id)`, the engine **must** produce the same `PolicyDecision`. This is a hard contract, not best-effort.

The triple is recorded on every decision and verified by audit tooling.

### 13.2. Caching

Decisions are cacheable per `(request_hash, bundle_version)` for the duration of `Constraints.ttl_seconds`:

- Same request from the same envelope re-submission within TTL ⇒ cached decision returned.
- Bundle flip ⇒ all cached decisions for the old bundle invalidated.
- Enrichment changes that would alter the decision ⇒ TTL must be respected; a fresh evaluation is required after TTL expiry.

The `Constraints.ttl_seconds` default is 300 seconds, max 3600 seconds, capped per rule.

### 13.3. Cache key formula

```text
cache_key = "polc_" + hex_lower(BLAKE3(JCS({
  request_hash,
  bundle_version
})))[:32]
```

## 14. Simulation

`SimulatePolicy(envelope) returns PolicyDecision` runs the full evaluation pipeline:

- Sets `simulated = true` on the result.
- Emits evidence marked `simulated = true` (per S3.1; production audit may filter out simulated entries).
- Never grants durable approval (any `REQUIRE_APPROVAL` outcome is simulation-only).
- Never modifies state.
- Bound by the same performance budgets as `EvaluatePolicy`.

Simulation is what powers Adaptive Backend pipeline (DEC-001) policy checks before real submission.

## 15. Approval boundary

This spec defines:

- When approval is required (rule `approval.required = true` OR triggered by AI self-approval prevention §17 OR triggered by `Constraints.require_human_co_signer = true`).
- How approval binds: to **exact** `request_hash`. Mutating the request invalidates the approval.
- Approval TTL: bounded by `Constraints.ttl_seconds`.
- Approval evidence linkage: `ApprovalReceipt.policy_decision_id` references the decision; decision references the approval receipt once granted.
- Who can approve: `approver_classes` filter (default `["human"]`).
- Approval cannot mutate the request.
- Approval cannot bypass hard denies.

Delivery, UI, multi-channel routing, and prompt rendering are deferred to **`04_approval_mechanics.md`**.

## 16. Emergency override boundary

Emergency override exists for situations where a scoped policy must be relaxed temporarily by a human operator (e.g., during incident response).

### 16.1. What override CAN bypass

- Specific scoped DENY rules (when emergency-override grant explicitly references the rule).
- Specific scoped REQUIRE_APPROVAL rules (downgrade to ALLOW with extra evidence).

### 16.2. What override CANNOT bypass

- Hard denies (§6).
- Evidence log mutation prohibitions.
- Recovery path protections (when not in recovery mode itself).
- AI self-approval prevention (§17) — only humans can override AI-affecting rules.

### 16.3. Required properties

- Override is **scoped**: identifies the rule(s) being overridden, the subject(s), the duration.
- Override is **time-bounded**: maximum 24 hours per grant; renewable but each renewal is a new evidence-logged grant.
- Override is **human-only**: only `subject_type = human` may issue.
- Override is **evidence-linked**: every override grant emits a receipt; every decision under override references the override receipt.
- Override grants do **not** persist across bundle versions; a bundle flip invalidates active grants.

### 16.4. Skeleton

Full mechanics (request flow, approver chain, audit) are in **`05_emergency_override.md`**. This sub-spec only sets the boundary.

## 17. AI subject self-approval prevention

Formal invariant. Cannot be disabled by policy bundle.

### 17.1. Rule

```text
IF  subject.is_ai = true
AND ( request.risk.destructive
   OR request.risk.privileged
   OR request.risk.network_exposure
   OR request.risk.secret_access
   OR request.risk.recovery_path_affected )
THEN  decision is upgraded to REQUIRE_APPROVAL
AND   approval.approver_classes must include "human" (and exclude AI types)
```

### 17.2. Application order

This rule runs **after** §5 step 5 (scoped allows) and may upgrade an `ALLOW` to `REQUIRE_APPROVAL`. It cannot downgrade a `DENY`.

### 17.3. Exception

The only exception is **self-management low-risk actions**: AI subjects may self-approve actions where all risk flags are `false` (e.g., `service.status`, `aiosfs.object.read` on PUBLIC objects).

### 17.4. Why this is hard-coded

The Cognitive Core may propose any action, including against itself. Without this invariant, a compromised AI subject could approve its own privileged actions. Hard-coding the rule ensures policy bundle authors cannot accidentally (or maliciously) introduce a bypass.

## 18. Performance contract

### 18.1. Budgets per call

| Path                                      | p95      | Hard timeout |
| ----------------------------------------- | -------- | ------------ |
| `EvaluatePolicy` (no enrichment, cached)  | < 1 ms   | 50 ms        |
| `EvaluatePolicy` (no enrichment, fresh)   | < 5 ms   | 50 ms        |
| `EvaluatePolicy` (with object enrichment) | < 25 ms  | 200 ms       |
| `SimulatePolicy`                          | < 50 ms  | 500 ms       |
| `LoadBundle` (validation + indexing)      | < 2 s    | 30 s         |
| `RollbackBundle`                          | < 500 ms | 5 s          |

### 18.2. Failure modes

- Hard timeout reached → `DENY` with `reason_code = PolicyEvaluationTimeout`.
- Enrichment unavailable → `DENY` with `reason_code = EnrichmentUnavailable`.
- Bundle signature failure → engine in degraded mode (§12.3); all evaluations except hard denies and emergency override return `DENY`.
- Internal engine error → `DENY` with `reason_code = PolicyEngineInternal`; evidence emits.

Engine fails closed by construction.

### 18.3. Backpressure

When evaluation queue exceeds threshold:

- Cached decisions still served.
- Fresh evaluations are throttled; eventual rejection with `RESOURCE_EXHAUSTED` if backpressure persists > 5 s.
- Hard denies always evaluated (cheap, no enrichment).

## 19. Adversarial robustness

### 19.1. Bundle load checks

Before activating a new bundle, the engine validates:

- **Cycle detection:** rules referencing each other (e.g., via subject groups expanding into other rules). Detected via dependency graph DFS; cycles cause `InvalidPolicyBundle`.
- **Rule complexity bounds:** each rule has ≤ 50 predicates; each `in [...]` clause has ≤ 100 values; total rules per bundle ≤ 10 000. Exceeded → `InvalidPolicyBundle`.
- **Field validation:** every `field` in conditions matches the §9.2 vocabulary.
- **Constraint validation:** every `constraint` matches §10 vocabulary with valid value type.
- **Subject reference validation:** every group reference resolvable in L4 identity.

### 19.2. Per-evaluation budget

- **Rule lookup budget:** default 1 000 rule lookups per evaluation; exceeded → `DENY` with `reason_code = PolicyEvaluationBudgetExceeded`.
- **Memory budget:** 64 MB per evaluation; exceeded → `DENY`.
- **Enrichment budget:** ≤ 16 object reads per evaluation; exceeded → `DENY`.

These bounds are also a defense against malicious or buggy rules.

### 19.3. Rate limits

Per-subject rate limit on `EvaluatePolicy` to prevent enumeration/probing:

- Default 1 000 evaluations/minute per subject.
- Exceeded → response delayed (token bucket); persistent abuse → `RESOURCE_EXHAUSTED`.

### 19.4. Decision integrity

Decisions are emitted with `evidence_receipt_id`; the evidence record contains the canonical input triple. Any party can re-run the evaluation with the same triple and verify the decision matches.

## 20. gRPC service surface

```proto
service PolicyKernel {
  rpc EvaluatePolicy(EvaluatePolicyRequest) returns (PolicyDecision);
  rpc SimulatePolicy(EvaluatePolicyRequest) returns (PolicyDecision);
  rpc LoadBundle(LoadBundleRequest) returns (LoadBundleResponse);
  rpc RollbackBundle(RollbackBundleRequest) returns (RollbackBundleResponse);
  rpc GetPolicyEngineInfo(google.protobuf.Empty) returns (PolicyEngineInfo);
  rpc ExplainDecision(ExplainDecisionRequest) returns (ExplainDecisionResponse);
}
```

`ExplainDecision` returns the rule chain that produced a given decision (subject to caller's privacy ceiling for any referenced objects).

Full message types in **Appendix A**.

## 21. Acceptance criteria

- Default deny works (action with no matching rule denied).
- Hard denies override all allow rules.
- Request mutation invalidates bound approval.
- `SimulatePolicy` produces decision with `simulated=true` and never grants durable approval.
- Decision is deterministic given the same `(request_hash, bundle_version, enrichment_snapshot_id)`.
- AI subjects cannot self-approve any action with a true risk flag (§17).
- Bundle signature failure puts engine in degraded mode (only hard denies + emergency override).
- Bundle with cyclic rule references is rejected.
- Per-evaluation rule budget is enforced.
- All golden fixtures from §22 pass against the implementation.
- Telemetry metrics from §23 are emitted with bounded label cardinality.
- Decision's evidence chain is reconstructible from the evidence log alone.

## 22. Golden decision fixtures

### 22.1. Scoped allow + verification required

```yaml
fixture_id: pk.fix.scoped_allow.v1
input_envelope:
  request:
    action: "service.restart"
    target: { service: "nginx" }
    subject: "human:lucky"
    risk: { privileged: true }
    environment: LOCAL
bundle:
  - rule_id: allow_restart_user_services
    effect: allow
    subjects: ["human:lucky"]
    actions: ["service.restart"]
    conditions:
      - 'environment = "LOCAL"'
      - 'target.service in ["nginx"]'
    constraints:
      sandbox_profile_id: host-service-control
      verification_required: true
expected:
  decision: ALLOW
  reason_code: ScopedAllow
  constraints.verification_required: true
  approval.required: false
```

### 22.2. Hard deny overrides scoped allow

```yaml
fixture_id: pk.fix.hard_deny_overrides.v1
input_envelope:
  request:
    action: "aiosfs.recursive_delete"
    target: { path: "/home" }
    subject: "human:lucky"
    risk: { destructive: true, privileged: true }
bundle:
  - rule_id: allow_lucky_anything
    effect: allow
    subjects: ["human:lucky"]
    actions: ["aiosfs.recursive_delete"]
expected:
  decision: DENY
  reason_code: hd.recursive_delete_root
  bypass_attempt_logged: true
```

### 22.3. AI self-approval prevention upgrades to require_approval

```yaml
fixture_id: pk.fix.ai_self_approval_blocked.v1
input_envelope:
  request:
    action: "package.install"
    subject: "agent:dev"
    risk: { privileged: true }
bundle:
  - rule_id: allow_dev_agent_install
    effect: allow
    subjects: ["agent:dev"]
    actions: ["package.install"]
expected:
  decision: REQUIRE_APPROVAL
  reason_code: AISelfApprovalPrevented
  approval.approver_classes: ["human"]
  approval.required: true
```

### 22.4. Approval bound to exact request hash

```yaml
fixture_id: pk.fix.request_mutation_invalidates.v1
scenario:
  - EvaluatePolicy(envelope_A) -> REQUIRE_APPROVAL
  - operator approves
  - envelope_B = mutated envelope_A (changed reason)
  - EvaluatePolicy(envelope_B) ignores approval; treated as new request
expected:
  envelope_A: ALLOW after approval
  envelope_B: REQUIRE_APPROVAL again (different request_hash)
  approval_a: not applicable to envelope_b
```

### 22.5. Decision determinism under same triple

```yaml
fixture_id: pk.fix.determinism.v1
scenario:
  - EvaluatePolicy(envelope_X) at time T1 -> decision_1
  - EvaluatePolicy(envelope_X) at time T2 (same bundle, same enrichment snapshot) -> decision_2
expected: decision_1.decision == decision_2.decision
  decision_1.constraints == decision_2.constraints
  decision_1.reason_code == decision_2.reason_code
```

### 22.6. Cyclic rule rejected at bundle load

```yaml
fixture_id: pk.fix.cycle_rejected.v1
bundle:
  - rule_id: r1
    subjects: ["group:a"]
  - rule_id: r2
    subjects: ["group:b"]
  group_definitions:
    a: { members: ["group:b"] }
    b: { members: ["group:a"] }
expected:
  load_status: InvalidPolicyBundle
  reason: "subject_group_cycle_detected"
  bundle_not_activated: true
```

### 22.7. Bundle signature failure enters degraded mode

```yaml
fixture_id: pk.fix.bundle_unsigned_degraded.v1
scenario:
  - bundle distributed without valid AIOS root signature
  - LoadBundle attempted
  - any non-hard-deny EvaluatePolicy issued
expected:
  load_status: SignatureInvalid
  engine_state: DEGRADED
  evaluation_decision: DENY (except hard denies which still apply)
  reason_code: PolicyEngineDegraded
```

### 22.8. Per-evaluation rule budget exceeded

```yaml
fixture_id: pk.fix.rule_budget_exceeded.v1
input_envelope:
  request: { action: "complex_action", ... }
bundle: 1500 matching rules with deep group expansion
expected:
  decision: DENY
  reason_code: PolicyEvaluationBudgetExceeded
  rules_consulted: 1000 # the budget cap
```

### 22.9. Emergency override scope honored

```yaml
fixture_id: pk.fix.emergency_override_scoped.v1
scenario:
  - operator creates emergency override grant for rule "deny_lan_exposure" with TTL 1 hour
  - EvaluatePolicy(envelope_with_lan_exposure) within TTL
expected:
  decision: ALLOW
  override_receipt_id_referenced: true
  evidence_marked_under_override: true
```

```yaml
fixture_id: pk.fix.emergency_override_cannot_bypass_hard_deny.v1
scenario:
  - operator creates emergency override grant referencing rule "hd.evidence_log_mutation"
expected:
  override_grant_status: Rejected
  reason: "hard_deny_cannot_be_overridden"
```

### 22.10. Bundle hot reload preserves in-flight decisions

```yaml
fixture_id: pk.fix.hot_reload_in_flight.v1
scenario:
  - EvaluatePolicy_A starts on bundle v1
  - bundle v2 loaded mid-evaluation
  - EvaluatePolicy_B starts on bundle v2
expected:
  EvaluatePolicy_A.bundle_version: v1
  EvaluatePolicy_B.bundle_version: v2
  no_evaluation_uses_mixed_versions: true
```

## 23. Telemetry contract

| Metric                                    | Type      | Labels                        |
| ----------------------------------------- | --------- | ----------------------------- |
| `policy_evaluations_total`                | counter   | `decision`, `reason_code`     |
| `policy_evaluation_latency_seconds`       | histogram | `decision`, `with_enrichment` |
| `policy_cache_hit_total` / `_miss_total`  | counter   |                               |
| `policy_bundle_loads_total`               | counter   | `outcome`                     |
| `policy_bundle_active_version`            | gauge     | `version`                     |
| `policy_engine_degraded`                  | gauge     |                               |
| `policy_emergency_override_active`        | gauge     |                               |
| `policy_hard_deny_total`                  | counter   | `policy_id`                   |
| `policy_ai_self_approval_blocked_total`   | counter   |                               |
| `policy_evaluation_budget_exceeded_total` | counter   | `budget_kind`                 |
| `policy_rules_consulted`                  | histogram |                               |
| `policy_simulations_total`                | counter   | `decision`                    |

Cardinality bounds: `decision` = 4, `reason_code` ≤ 30 documented codes, `outcome` ≤ 4, `policy_id` bounded to hard-deny set (~10), `budget_kind` ≤ 4, `version` bounded to recent active versions (≤ 5). Subject is **never** a metric label.

## 24. Cross-spec dependencies

| Spec                               | Relationship                                                                     |
| ---------------------------------- | -------------------------------------------------------------------------------- |
| **S0.1** Action Envelope           | `request_hash` and identity binding inherited; envelope is the input.            |
| **S1.1** Capability Translator     | Translator's REJECTED is structural; this kernel's DENY is governance. Distinct. |
| **S1.2** Latency Tiering           | Routing privacy ceilings flow into subject session class.                        |
| **S1.3** Object Model              | Resource enrichment uses AIOS-FS SNAPSHOT reads.                                 |
| **S2.1** Query DSL                 | Conditions vocabulary parallels query DSL grammar; same encoding rules.          |
| **S2.4** Verification Grammar      | Constraints reference verification grammar via `verification_required`.          |
| **S3.1** Evidence Log              | Every decision and override emits evidence; chain is reconstructible.            |
| **L4 Identity Model (`03_…`)**     | Subject hydration source.                                                        |
| **L4 Approval Mechanics (`04_…`)** | Approval delivery and prompt rendering.                                          |
| **L4 Emergency Override (`05_…`)** | Override mechanics, request flow, audit.                                         |
| **L6 Sandbox Composition (S3.2)**  | Constraint `sandbox_profile_id` references profiles defined here.                |

## 25. Open deferrals

- Approval delivery, UI, multi-channel routing → `04_approval_mechanics.md`.
- Emergency override request flow, approver chain, audit details → `05_emergency_override.md`.
- Identity hydration internals (group resolution, capability propagation) → `03_identity_model.md`.
- Vault capability checks (`vault_capability_required`) → `02_vault_broker.md`.
- OPA/Rego or CEL backend evaluator → future revision; canonical AIOS schema is rev.2 authority.
- Distributed multi-instance policy consensus → future revision; rev.2 assumes single authoritative engine per host.
- Policy authoring IDE / linter → tooling, out of scope.

## 26. See also

- [S0.1 Action Envelope + Lifecycle](../../002.AI-OS.NET--SPECREV.2/XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S1.3 Object Model](../L2_AIOS_FS/01_object_model.md)
- [S2.1 Query/View Language](../L2_AIOS_FS/02_query_view_language.md)
- [L4 overview](00_overview.md)
- [Rev.1 §11 — Policy Kernel](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

## Appendix A: Complete proto IDL

```proto
syntax = "proto3";
package aios.policy.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/struct.proto";
import "google/protobuf/empty.proto";

// ─────────────────────────────────────────────────────────────────
// Decision and constraints
// ─────────────────────────────────────────────────────────────────

enum Decision {
  DECISION_UNSPECIFIED = 0;
  ALLOW                = 1;
  REQUIRE_APPROVAL     = 2;
  DENY                 = 3;
}

message Constraints {
  string sandbox_profile_id = 1;
  uint32 max_runtime_seconds = 2;
  bool verification_required = 3;
  bool dry_run_only = 4;
  string require_evidence_grade = 5;
  bool require_human_co_signer = 6;
  string network_policy = 7;
  uint32 max_concurrent_per_subject = 8;
  string min_subject_session_class = 9;
  string vault_capability_required = 10;
  uint32 ttl_seconds = 11;
}

message ApprovalRequirement {
  bool required = 1;
  string approval_scope = 2;
  uint32 ttl_seconds = 3;
  repeated string approver_classes = 4;
  bool require_human_co_signer = 5;
}

message PolicyDecision {
  string policy_decision_id = 1;
  string action_id = 2;
  string request_hash = 3;
  string bundle_version = 4;
  string enrichment_snapshot_id = 5;
  Decision decision = 6;
  string reason_code = 7;
  string reason_message = 8;
  Constraints constraints = 9;
  ApprovalRequirement approval = 10;
  string evidence_receipt_id = 11;
  google.protobuf.Timestamp evaluated_at = 12;
  uint32 rules_consulted = 13;
  bool simulated = 14;
}

// ─────────────────────────────────────────────────────────────────
// Rules and bundles
// ─────────────────────────────────────────────────────────────────

enum RuleEffect {
  RULE_EFFECT_UNSPECIFIED = 0;
  ALLOW_EFFECT = 1;
  DENY_EFFECT  = 2;
}

message PolicyRule {
  string rule_id = 1;
  RuleEffect effect = 2;
  int32 priority = 3;
  repeated string subjects = 4;
  repeated string actions = 5;
  repeated string conditions = 6;
  Constraints constraints = 7;
  ApprovalRequirement approval = 8;
  RuleMetadata metadata = 9;
}

message RuleMetadata {
  string description = 1;
  repeated string authors = 2;
  string policy_pack = 3;
  google.protobuf.Timestamp created_at = 4;
  google.protobuf.Timestamp last_modified_at = 5;
}

message HardDenyEntry {
  string policy_id = 1;
  string description = 2;
  string override_path = 3;          // empty = no override
}

message PolicyBundle {
  string bundle_version = 1;          // "polb_<hex_lower(BLAKE3(...))[:32]>"
  string schema_version = 2;          // "aios.policy.v1alpha1"
  repeated PolicyRule rules = 3;
  repeated HardDenyEntry hard_denies = 4;
  google.protobuf.Struct group_definitions = 5;   // group->subjects map
  string publisher_id = 6;
  google.protobuf.Timestamp created_at = 7;
  bytes publisher_signature = 8;
  bytes aios_root_signature = 9;
}

// ─────────────────────────────────────────────────────────────────
// RPC surface
// ─────────────────────────────────────────────────────────────────

message EvaluatePolicyRequest {
  string schema_version = 1;          // "aios.policy.v1alpha1"
  bytes envelope_proto = 2;           // serialized aios.action.v1alpha1.ActionEnvelope
}

message LoadBundleRequest {
  PolicyBundle bundle = 1;
  bool stage_only = 2;                // if true, validates but doesn't activate
}

message LoadBundleResponse {
  string bundle_version = 1;
  bool active = 2;
  string status_message = 3;
  google.protobuf.Timestamp activated_at = 4;
}

message RollbackBundleRequest {
  string target_bundle_version = 1;
  string operator_subject = 2;
  string reason = 3;
}

message RollbackBundleResponse {
  string previous_bundle_version = 1;
  string current_bundle_version = 2;
  string evidence_receipt_id = 3;
}

message ExplainDecisionRequest {
  string policy_decision_id = 1;
}

message ExplainDecisionResponse {
  PolicyDecision decision = 1;
  repeated string rule_chain = 2;     // ordered rule_ids that contributed
  string narrative = 3;               // human-readable plain text
  google.protobuf.Struct enrichment_snapshot = 4;
}

message PolicyEngineInfo {
  string engine_id = 1;
  repeated string supported_schema_versions = 2;
  string default_schema_version = 3;
  string active_bundle_version = 4;
  bool degraded = 5;
  uint32 rules_in_active_bundle = 6;
  google.protobuf.Timestamp started_at = 7;
}

service PolicyKernel {
  rpc EvaluatePolicy(EvaluatePolicyRequest) returns (PolicyDecision);
  rpc SimulatePolicy(EvaluatePolicyRequest) returns (PolicyDecision);
  rpc LoadBundle(LoadBundleRequest) returns (LoadBundleResponse);
  rpc RollbackBundle(RollbackBundleRequest) returns (RollbackBundleResponse);
  rpc ExplainDecision(ExplainDecisionRequest) returns (ExplainDecisionResponse);
  rpc GetPolicyEngineInfo(google.protobuf.Empty) returns (PolicyEngineInfo);
}
```
