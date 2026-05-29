# S27 - AI Evaluation and Model Governance

| Field     | Value                                                                                                                                                                                                                 |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-29; awaiting implementation evidence)                                                                                                                                                     |
| Phase tag | S27                                                                                                                                                                                                                   |
| Layer     | Cross-cutting: L5, L0 crossing L4, L9                                                                                                                                                                                 |
| Consumes  | S20 Native AI Control Plane and AI Terminal, S13.1 Cognitive Core Model, S13.2 Model Router, S1.1 Capability Translator, S3.1 Evidence Log, S2.3 Policy Kernel, S16.3 STIG/NIST Control Map + Scanner                 |
| Produces  | `AIEvaluationHarness`, `ModelEvaluationReport`, `FederatedModelMarketplace`, `SignedModelBundle`, `PublicBenchmarkContract`, `MultiAgentCoordination`, `AgentRole`, model-evaluation and multi-agent evidence records |

## 1. Responsibility

S27 makes S20's accuracy and robustness claims **verifiable** instead of
asserted, and it gives Rev.3 a constitutional home for two cognitive-expansion
themes the planning notes flagged but Rev.2 left open: the federated model
marketplace and multi-agent coordination.

S20 §10 lists an `Accuracy/robustness` AI Act control family whose mechanism is
"verification plan, confidence/uncertainty field, post-action checks, drift
monitoring." Today that family has **no evidence record types** — there is no
way to prove an accuracy claim, a measured drift, a hallucination rate, or a
prompt-injection rejection rate. S27 closes that gap by defining the missing
evidence records and the harness that emits them.

S27 also extends the single Rev.2 `CognitiveCore` into a coordinated set of
distinct AI subjects (planner, executor, reviewer) where the reviewer is a
different subject than the executor, so the constitutional rule "AI cannot grade
its own work" (INV-016) holds across agents, not just within one.

Invariant links: INV-002 (AI proposes, never executes), INV-010 (AI cannot
self-approve), INV-014 (no proof, no completion), INV-015 (evidence never
contains secrets), and INV-016 (AI cannot grade its own work). The cross-agent
no-self-grade rule defined in §7 is a **specialization** of INV-016 + INV-010 +
INV-002 across distinct AI subjects; it introduces no new invariant number.

## 2. Product principle

A model claim is only as good as the evidence behind it. AIOS must be able to
answer, for any model it runs:

```text
how accurate is it on a fixed benchmark
is it drifting from its last accepted baseline
how often does it hallucinate under a fixed probe set
how often does it reject a prompt-injection attempt
is it well calibrated (does stated confidence match observed correctness)
who measured this, when, and against which signed benchmark
```

The product promise mirrors the holistic solver pattern (holistic §6): an
evaluation is a request that inspects signed state, runs against a fixed signed
benchmark **off the active decision path**, scores benefit/risk/compatibility
against the previous baseline, applies policy, and promotes a model only with
evidence — or blocks it with a reason.

```text
model registered (S13.2 AIModelRegistryEntry)
  -> S27 AIEvaluationHarness run against signed benchmark
  -> ModelEvaluationReport (accuracy, drift, hallucination, injection, calibration)
  -> policy gate against profile thresholds
  -> promote model to eligible-for-routing, or block with reason
  -> evidence
```

Technical evaluation is a **measurement**, never a **safety guarantee** (see
§13 Non-goals).

## 3. Reference patterns

| Pattern                                                                                                          | S27 use                                                                                                                |
| ---------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| [NIST AI Risk Management Framework (AI RMF 1.0)](https://www.nist.gov/itl/ai-risk-management-framework)          | Measure/Manage functions; map evaluation evidence to risk-management activities.                                       |
| [EU AI Act regulatory framework](https://digital-strategy.ec.europa.eu/en/policies/regulatory-framework-ai)      | Accuracy, robustness, and cybersecurity expectations for the technical-documentation control family (shared with S20). |
| [OWASP Top 10 for LLM Applications](https://owasp.org/www-project-top-10-for-large-language-model-applications/) | Prompt-injection (LLM01) probe taxonomy and rejection-rate measurement.                                                |
| [Model Cards (Mitchell et al.)](https://arxiv.org/abs/1810.03993)                                                | Structured per-model reporting fields carried in `ModelEvaluationReport`.                                              |
| [Calibration of modern neural networks (Guo et al.)](https://arxiv.org/abs/1706.04599)                           | Expected Calibration Error (ECE) as the calibration metric.                                                            |
| [in-toto attestation framework](https://in-toto.io/)                                                             | Signed provenance binding a benchmark result to the exact model bundle and harness version.                            |
| [SLSA provenance levels](https://slsa.dev/)                                                                      | Supply-chain provenance level carried by a `SignedModelBundle`.                                                        |

## 4. Evaluation harness

`AIEvaluationHarness` is a typed, signed evaluation runner. It never sits on the
live decision path: it consumes a model bundle plus a signed benchmark and emits
a report. It cannot mutate the model, the registry, or the routing decision; it
only produces evidence that a policy gate reads.

```yaml
ai_evaluation_harness:
  harness_id: "evalh_<ULID>"
  harness_version: "2026.05.rev3"
  evaluator_subject: "subject:system_service:s27-eval" # SYSTEM_SERVICE, never AI
  model_under_test:
    model_id: "model:local:example" # S13.2 AIModelRegistryEntry id
    bundle_ref: "smb_<ULID>" # SignedModelBundle id, §10
    bundle_digest: "sha256:..."
  benchmark:
    benchmark_id: "bench:aios:assist-core:v3"
    benchmark_digest: "sha256:..."
    signature_chain: []
    kind: ACCURACY | DRIFT | HALLUCINATION | PROMPT_INJECTION | CALIBRATION
    sample_count: 0
    frozen: true # benchmark contents are immutable
  baseline:
    baseline_report_id: "mer_<ULID>" # previous accepted ModelEvaluationReport
    baseline_digest: "sha256:..."
  isolation:
    off_active_path: true # evaluation cannot affect live routing
    no_external_calls_in_airgap: true
  evidence:
    report_receipt: "evr_..."
```

Harness rules:

```text
the evaluator subject is SYSTEM_SERVICE, never an AI subject
the model under test cannot be the evaluator
the benchmark is frozen and signed; its digest is recorded in the report
the harness runs off the active decision path
the harness only emits evidence; it never promotes a model itself
in AIRGAP_HIGH the harness uses local benchmarks only, no external calls
```

```text
EvaluationHarnessState =
  STAGED
| RUNNING
| REPORT_EMITTED
| BLOCKED_BENCHMARK_UNVERIFIED
| FAILED
```

Unknown values are rejected by the harness manifest loader.

## 5. Model evaluation report

`ModelEvaluationReport` is the single closed schema that carries every measured
metric for one model against one benchmark run. It is append-only output: a new
run produces a new report; reports are never edited in place.

```yaml
model_evaluation_report:
  report_id: "mer_<ULID>"
  model_id: "model:local:example"
  bundle_digest: "sha256:..."
  harness_id: "evalh_<ULID>"
  harness_version: "2026.05.rev3"
  benchmark_id: "bench:aios:assist-core:v3"
  benchmark_digest: "sha256:..."
  evaluated_at: "RFC3339"
  evaluator_subject: "subject:system_service:s27-eval"
  metrics:
    accuracy:
      score: 0.0 # 0.0..1.0 on the frozen benchmark
      n: 0
      confidence_interval_95: [0.0, 0.0]
    drift:
      baseline_report_id: "mer_<ULID>"
      delta_accuracy: 0.0 # signed; negative = regression
      drift_detected: false
      drift_threshold: 0.05
    hallucination:
      probe_set_id: "probe:halluc:v2"
      rate: 0.0 # fraction of unsupported/fabricated answers
      n: 0
    prompt_injection:
      probe_set_id: "probe:owasp-llm01:v2"
      rejection_rate: 0.0 # fraction of injection attempts correctly refused
      attempts: 0
      successful_injections: 0
    calibration:
      metric: ECE # Expected Calibration Error
      ece: 0.0 # lower is better
      bins: 10
  outcome:
    verdict: PASS | FAIL | WARN | EXCEPTION
    profile_thresholds_ref: "s27.thresholds.<profile>"
    blocked_reason: null
  evidence_receipt_id: "evr_..."
```

The five metric families are the closed set S27 measures:

```text
EvaluationMetricKind =
  ACCURACY
| DRIFT
| HALLUCINATION
| PROMPT_INJECTION_REJECTION
| CALIBRATION
```

Unknown values are rejected by the report validator.

## 6. Federated model marketplace

`FederatedModelMarketplace` is the distribution surface for models beyond the
vault-brokered external path (S13.2). It does **not** introduce a new execution
authority: a marketplace bundle is still routed through S13.2 and still subject
to S2.3 policy. The marketplace adds signed bundles and public benchmark
contracts so a model's claimed quality is checkable before it is trusted.

```yaml
signed_model_bundle:
  bundle_id: "smb_<ULID>"
  model_id: "model:vendor:example"
  publisher: "publisher:example"
  trust_level: AIOS_VERIFIED | THIRD_PARTY_SIGNED | LOCAL_ONLY | UNTRUSTED
  artifact_digest: "sha256:..."
  signature_chain: []
  sbom_ref: "optional" # supply-chain link (assumes S16.6 SBOM/Provenance/VEX)
  provenance_ref: "optional" # in-toto / SLSA provenance attestation
  slsa_level: 0 # 0..4
  declared_benchmarks:
    - benchmark_id: "bench:public:assist-core:v3"
      benchmark_digest: "sha256:..."
      publisher_claimed_report_ref: "mer_<ULID>"
  license: "open|proprietary|dual|unknown"
  evidence:
    publish_receipt: "evr_..."
```

```yaml
public_benchmark_contract:
  benchmark_id: "bench:public:assist-core:v3"
  benchmark_digest: "sha256:..."
  owner: "publisher:example"
  frozen: true
  sample_count: 0
  metric_kinds: [ACCURACY, HALLUCINATION, PROMPT_INJECTION_REJECTION]
  reproducible: true # AIOS can re-run it locally and reproduce the score
  signature_chain: []
```

Marketplace rules:

```text
a publisher's claimed score is a claim, not a result
AIOS re-runs the public benchmark locally through AIEvaluationHarness
the local ModelEvaluationReport, not the publisher claim, gates promotion
an UNTRUSTED bundle is never eligible for routing under any profile
no model bundle is promoted without a benchmark digest match
```

## 7. Multi-agent coordination

`MultiAgentCoordination` extends the single Rev.2 `CognitiveCore` into three
**distinct** AI subjects. Each agent is an identifiable `AI_AGENT_CAPSULE`
subject (S20 §5 actor kind) with its own subject id, tool grants, and evidence
trail. The roles are a closed set:

```text
AgentRole =
  PLANNER     # decomposes intent into a candidate plan; proposes typed actions
| EXECUTOR    # carries out approved typed actions through the Capability Runtime
| REVIEWER    # grades whether the executor's output meets the plan's acceptance
```

Unknown values are rejected by the coordination admission check.

The constitutional separation rule (the cross-agent form of INV-016 + INV-010 +
INV-002, per §8):

```text
the REVIEWER subject MUST NOT be the same subject as the EXECUTOR subject
the REVIEWER MUST NOT be the same subject as the PLANNER for the same task
each agent has a distinct subject_id and distinct tool grants
no agent self-approves; approval still belongs to S2.3 Policy Kernel
no agent grades its own completion proof
```

```yaml
multi_agent_coordination:
  coordination_id: "mac_<ULID>"
  task_intent_id: "intent_<ULID>"
  orchestrator_subject: "subject:system_service:cognitive-core" # SYSTEM_SERVICE
  agents:
    planner:
      subject_id: "subject:ai_agent_capsule:planner-<id>"
      actor_kind: AI_AGENT_CAPSULE
      model_id: "model:local:example"
    executor:
      subject_id: "subject:ai_agent_capsule:executor-<id>"
      actor_kind: AI_AGENT_CAPSULE
      model_id: "model:local:example"
    reviewer:
      subject_id: "subject:ai_agent_capsule:reviewer-<id>"
      actor_kind: AI_AGENT_CAPSULE
      model_id: "model:local:example"
  separation:
    reviewer_distinct_from_executor: true # enforced; false is rejected at admission
    reviewer_distinct_from_planner: true
  review_outcome:
    verdict: ACCEPTED | REJECTED | NEEDS_REWORK
    grounded_on_evidence_ref: "evr_..."
  evidence:
    coordination_receipt: "evr_..."
```

```text
MultiAgentState =
  PLANNING
| AWAITING_APPROVAL          # plan leaves the agents and enters S2.3
| EXECUTING
| UNDER_REVIEW
| ACCEPTED
| REJECTED
| BLOCKED_SEPARATION_VIOLATION
```

Unknown values are rejected by the coordination admission check. A coordination
whose reviewer subject equals its executor subject is rejected at admission and
emits `AGENT_REVIEW_SEPARATION_ENFORCED` with the violation, enforcing the
cross-agent specialization of INV-016 + INV-010 + INV-002 described in §8.

This is the home for DEC-R3-011's "multi-agent coordination → S20 (added
section)" routing: S27 defines the coordination contract and S20 references it as
the multi-agent extension of its actor model.

## 8. Invariant mapping (no new invariant)

INV-016 ("AI cannot grade its own work") and INV-010 ("AI cannot self-approve")
were written for a single core. Rev.3's multi-agent model needs them to hold
across distinct AI subjects. S27 introduces **no new constitutional rule**: the
multi-agent separation rule is a **specialization** of the inherited invariants
applied across subjects, exactly as the Rev.3 invariant catalog
(`04_invariants.md` §2) requires multi-agent/eval rules to be mapped rather than
minted.

```text
cross-agent no-self-grade  = INV-016 across subjects
                             (a reviewer AI subject cannot grade the same AI
                              subject that produced the output)
cross-agent no-self-approve = INV-010 across subjects
                             (no agent approves its own action; approval stays
                              with the S2.3 Policy Kernel)
distinct-subject identity   = INV-002 + S20 §5 actor-kind rules
                             (each agent is a distinct AI_AGENT_CAPSULE subject;
                              AI proposes, never executes its own grade/approval)
```

This is why S27 does **not** allocate an `InvariantId`: INV-028 in
`04_invariants.md` is owned by S16.4 (boot-integrity authorship), and the
cross-agent rule is fully covered by the inherited INV-016 + INV-010 + INV-002.
Any earlier draft that provisionally numbered this rule `INV-028` is corrected
here per the collision resolution in `04_invariants.md` §1.

## 9. Security profile gates

| Profile          | Evaluation / governance rule                                                                                                                                                                                                                          |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `DEV_RELAXED`    | Evaluation reports may be informational; thresholds advisory; `LOCAL_ONLY` and unverified bundles allowed with warning.                                                                                                                               |
| `SECURE_DEFAULT` | A model is routing-eligible only after a `ModelEvaluationReport` with `verdict = PASS`; drift beyond threshold emits a WARN and requires re-evaluation.                                                                                               |
| `STIG_ALIGNED`   | Routing-eligible only with a current PASS report against a signed benchmark; `MODEL_DRIFT_DETECTED` forces re-evaluation before continued use; `UNTRUSTED` and unsigned bundles blocked; prompt-injection rejection rate must meet the profile floor. |
| `AIRGAP_HIGH`    | Local benchmarks only; no external benchmark fetch, no marketplace download; every promotion requires a locally reproduced report; exceptions are recovery-approved.                                                                                  |

Hard denies (enforced by S2.3 across `STIG_ALIGNED` and `AIRGAP_HIGH`):

```text
no AI subject may author, run, or sign its own evaluation
no AI subject may approve a model promotion
no model is promoted on a publisher-claimed score without a locally reproduced report
no benchmark may be mutated to change a verdict (frozen + digest checked)
no reviewer subject may equal the executor subject it grades
no evaluation evidence may be edited or deleted (append-only)
```

## 10. Evidence records

S27 adds these record types. The first five are exactly the records S20's
accuracy/robustness control family was missing:

```text
MODEL_EVAL_COMPLETED
MODEL_DRIFT_DETECTED
HALLUCINATION_RATE_RECORDED
PROMPT_INJECTION_REJECTION_MEASURED
MODEL_BENCHMARK_PUBLISHED
MODEL_EVAL_STARTED
MODEL_EVAL_BLOCKED
CALIBRATION_RECORDED
MODEL_BUNDLE_SIGNED
MODEL_BUNDLE_VERIFIED
MODEL_PROMOTION_GATED
MODEL_PROMOTION_BLOCKED
AGENT_ROLE_ASSIGNED
AGENT_REVIEW_RECORDED
AGENT_REVIEW_SEPARATION_ENFORCED
```

Minimum fields for `MODEL_EVAL_COMPLETED`:

```text
report_id
model_id
bundle_digest
harness_id
harness_version
benchmark_id
benchmark_digest
evaluator_subject
accuracy_score
drift_delta
hallucination_rate
prompt_injection_rejection_rate
calibration_ece
verdict
security_profile
evidence_receipt_id
```

Minimum fields for `MODEL_DRIFT_DETECTED`:

```text
report_id
model_id
baseline_report_id
metric_kind
delta_accuracy
drift_threshold
security_profile
evidence_receipt_id
```

Evidence never contains prompt bodies, secret material, or raw private benchmark
inputs (INV-015); only metric values, digests, and ids are recorded.

## 11. Operator experience

The operator sees a Model Passport, not raw eval logs. Minimum fields:

- model name, bundle digest, trust level
- last evaluation date and benchmark id
- accuracy score and 95% interval
- drift since baseline (and whether drift was detected)
- hallucination rate and prompt-injection rejection rate
- calibration (ECE)
- routing eligibility under the active security profile
- which agent subjects (planner/executor/reviewer) ran a task
- evidence receipt

One-click operator actions (each maps to a typed policy decision; the UI is not
authority):

```text
Re-evaluate model
Compare to baseline
Promote model (if PASS and policy allows)
Block model
View benchmark contract
View agent review
```

## 12. Acceptance criteria

S27 is `REAL` only when:

1. `AIEvaluationHarness` runs a model against a frozen, signed benchmark off the
   active decision path and rejects an unverified benchmark.
2. `ModelEvaluationReport` is emitted with all five metric families and rejects
   unknown `EvaluationMetricKind` values.
3. `MODEL_EVAL_COMPLETED`, `MODEL_DRIFT_DETECTED`,
   `HALLUCINATION_RATE_RECORDED`, `PROMPT_INJECTION_REJECTION_MEASURED`, and
   `MODEL_BENCHMARK_PUBLISHED` are produced and absorbed by the Evidence Log.
4. The evaluator subject is a `SYSTEM_SERVICE`, never an AI subject, and an AI
   subject cannot author, run, or sign its own evaluation.
5. A publisher-claimed score never promotes a model; only a locally reproduced
   `ModelEvaluationReport` gates promotion.
6. An `UNTRUSTED` or unsigned `SignedModelBundle` is blocked from routing under
   every profile.
7. Drift beyond threshold under `STIG_ALIGNED` forces re-evaluation before
   continued use.
8. `MultiAgentCoordination` admits planner/executor/reviewer as distinct
   `AI_AGENT_CAPSULE` subjects and rejects any coordination whose reviewer
   subject equals the executor subject (INV-016 across subjects).
9. No agent self-approves; approval remains with the S2.3 Policy Kernel.
10. All evaluation and review evidence is append-only and free of secret or raw
    prompt material.

## 13. Non-goals

- Do not treat a passing evaluation as a safety guarantee or legal conformity;
  it is a measurement against a fixed benchmark.
- Do not let the evaluation harness promote a model by itself; promotion is a
  policy decision.
- Do not let an AI subject grade, sign, or approve its own evaluation.
- Do not store prompts, secrets, or raw private benchmark inputs in evidence.
- Do not claim a model is "accurate" or "robust" without a current signed
  benchmark report.
- Do not let the marketplace introduce a routing path that bypasses S13.2 or
  S2.3.
- Do not let multi-agent coordination create a reviewer that is the same subject
  as the executor it reviews.

## 14. See also

- [S20 Native AI Control Plane and AI Terminal](../S20_Native_AI_Control_Plane_Terminal/00_overview.md)
- [S16 Security Hardening and Compliance](../S16_Security_Hardening_Compliance/00_overview.md)
- [S16.3 STIG/NIST Control Map + Scanner](../S16_Security_Hardening_Compliance/03_stig_nist_control_map_scanner.md)
- [Rev.3 Holistic Specification](../00_REV3_HOLISTIC_SPEC.md)
- [Rev.3 Design Decisions](../02_design_decisions.md)
- [S13.1 Cognitive Core Model](../../002.AI-OS.NET--SPECREV.2/L5_Cognitive_Core/01_cognitive_core_model.md)
- [S13.2 Model Router](../../002.AI-OS.NET--SPECREV.2/L5_Cognitive_Core/05_model_router.md)
- [S1.1 Capability Translator](../../002.AI-OS.NET--SPECREV.2/L5_Cognitive_Core/02_capability_translator.md)
- [S2.3 Policy Kernel](../../002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S3.1 Evidence Log (evidence receipt schema)](../../002.AI-OS.NET--SPECREV.2/L0_Governance_Evidence_Safety/03_evidence_receipt_schema.md)
