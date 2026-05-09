# Capability Runtime gRPC (Rev.2)

| Field          | Value                                                                                                                                                            |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists; structural contract complete)                                                                    |
| Phase tag      | S10.1                                                                                                                                                            |
| Layer          | L3 AIOS-SGR / Capability Runtime                                                                                                                                 |
| Schema package | `aios.runtime.v1alpha1`                                                                                                                                          |
| Consumes       | S0.1 Action Envelope + Lifecycle, S2.3 Policy Kernel, S5.3 Approval Mechanics, S5.4 Emergency Override, S2.4 Verification Engine, S3.2 Sandbox Composition, S3.1 |
|                | Evidence Log RecordType + retention vocabulary, L4.2 Vault Broker capability bindings, L0 invariant catalog                                                      |
| Produces       | typed `aios.runtime.v1alpha1.CapabilityRuntime` service surface, closed lifecycle FSM, closed adapter manifest schema, dispatch and queueing contracts, twenty   |
|                | evidence record types queued for S3.1, performance budgets, adversarial-robustness fixtures                                                                      |

## §1 Purpose

This sub-spec defines the **internal service surface** of the AIOS Capability Runtime — the L3 component that takes a typed `ActionEnvelope` (S0.1), runs it through a closed pre-dispatch sequence, hands it to a registered adapter under a closed dispatch kind, observes verification, and emits the full evidence chain.

The S0.1 envelope is the wire shape and the public lifecycle. S0.1 also specifies the public `aios.action.v1alpha1.CapabilityRuntime` ingress (`SubmitAction` / `WatchAction` / `GetAction` / `ListAdapters` / `GetAdapterCapabilities` / `GetCapabilityRuntimeInfo`). This sub-spec is **not** a redefinition of that public ingress. It defines the **L3-internal orchestration RPCs** that the public ingress calls into — the surface that materialises Rev.1 §13's nine-RPC orchestration model on top of the S0.1 envelope. The two surfaces are layered:

```text
S0.1 public ingress (aios.action.v1alpha1.CapabilityRuntime)
        |
        v
S10.1 internal orchestration (aios.runtime.v1alpha1.CapabilityRuntime)
        |
        v
adapter dispatch (aios.runtime.v1alpha1.AdapterManifest)
```

A `SubmitAction` call from outside L3 walks the S10.1 internal RPCs in order: `ValidateAction → EvaluatePolicyForAction → RequestApprovalForAction (conditional) → ExecuteAction → VerifyAction → (RollbackAction on failure)`. The S10.1 surface is also reachable directly by L3-internal callers (recovery diagnostics, operator admin tools, the override manager) for fine-grained orchestration.

This file defines:

1. The closed `ActionLifecycleState` enum and its allowed-transition table.
2. The closed gRPC service surface in `aios.runtime.v1alpha1` with proto IDL.
3. The closed `AdapterManifest` schema, signature discipline, and registration mechanics.
4. The closed `ActionDispatchKind`, `AdapterIOMode`, `AdapterStability`, `QueueClass`, `ExecutionFailureReason`, `RollbackOutcome`, and `RuntimeErrorCode` enums.
5. The strictly ordered pre-dispatch sequence that runs before `ExecuteAction` actually invokes the adapter.
6. The verification path that runs after adapter success, citing S2.4.
7. The closed rollback FSM and the `ROLLBACK_FAILED` terminal state.
8. The escape via S5.4 emergency override for hard-denied actions.
9. The dry-run contract that forces `DRY_RUN` dispatch and emits `SIMULATION` evidence.
10. The queueing and concurrency contract (`QueueClass`, per-subject rate limits, AI-share cap).
11. The adversarial robustness rules.
12. The closed list of evidence record types this sub-spec emits and their retention class (queued for S3.1).
13. The performance contract for each RPC.
14. Three worked examples that walk the FSM end-to-end.

This file does **not** define:

- The action envelope shape or canonical hash convention. That is S0.1.
- The policy evaluation pipeline or hard-deny vocabulary. That is S2.3.
- The approval prompt, channel selection, or binding scheme. That is S5.3.
- The override quorum, cooldown, or `NonOverridableClass`. That is S5.4.
- The verification primitive vocabulary, EBNF, or property checks. That is S2.4.
- The sandbox profile shape, composition algorithm, or backend probes. That is S3.2.
- The evidence log hash chain, segment lifecycle, or query API. That is S3.1.
- Adapter implementation patterns or per-adapter target schemas. That is `04_adapter_model.md` (out of scope for this sub-spec).

## §2 Scope

In scope:

- The `aios.runtime.v1alpha1.CapabilityRuntime` service surface — nine RPCs covering validation, policy orchestration, approval orchestration, dispatch, verification, rollback, status, adapter discovery, and runtime info.
- The closed `ActionLifecycleState` enum with fourteen states and a fully enumerated allowed-transition table.
- The closed `AdapterManifest` record with signature discipline.
- The closed `ActionDispatchKind`, `AdapterIOMode`, `AdapterStability`, `QueueClass`, `ExecutionFailureReason`, `RollbackOutcome`, `RuntimeErrorCode` enums.
- The strictly ordered eight-step pre-dispatch sequence.
- The verification handoff to S2.4.
- The rollback FSM with `ROLLBACK_FAILED` terminal state.
- The hard-deny escape via S5.4 override binding consumption.
- Dry-run contract (S0.1 `dry_run = SIMULATE`).
- Queueing, fairness, AI-share cap.
- Twenty evidence record types queued for S3.1.
- Performance budgets per RPC.
- Adversarial robustness rules covering idempotency replay, manifest forgery, envelope tampering, hash mismatch, kind-overrun, evidence chain failure, queue-class abuse, latency budget breach.
- Three worked examples end-to-end.

Out of scope:

- L3 service unit manifest (`01_unit_manifest.md`) — the SGR's desired-state schema.
- L3 desired-state evaluator (`02_state_transitions.md`) — graph evaluation, A/B promotion.
- L3 adapter implementation patterns (`04_adapter_model.md`) — per-adapter target schemas, per-adapter lifecycle hooks.
- The Cognitive Core's Capability Translator (S1.1) — emits envelopes; does not consume the L3 surface.
- The renderer trust path (L7) — relevant only because approval prompts render there; not authored here.
- Multi-host capability federation — single authoritative L3 instance per host in Rev.2.

## §3 Vocabulary

This section declares the closed enums on which the rest of the sub-spec is built. Every enum is contract-grade. Adding a value is a versioned spec change. Bundle load fails on unknown values. Wire compatibility is governed by the S0.1 §8 versioning policy.

### §3.1 `ActionLifecycleState`

Closed enum, fourteen states. This is the L3-internal lifecycle. The S0.1 public `Phase` enum (`PENDING`, `RUNNING`, `SUCCEEDED`, `FAILED`, `ROLLED_BACK`) is a five-bucket projection of this finer-grained state. The `Phase` is denormalised from `ActionLifecycleState` by the runtime per S0.1 §6.6 and exposed on the public envelope; the fourteen-state lifecycle is exposed only on the S10.1 surface.

| Value              | Phase projection (S0.1) | Semantics                                                                                                                                                           |
| ------------------ | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `CREATED`          | `PENDING`               | Envelope accepted by the runtime; pre-validation not yet complete.                                                                                                  |
| `POLICY_PENDING`   | `PENDING`               | `EvaluatePolicyForAction` in flight; awaiting Policy Kernel decision.                                                                                               |
| `APPROVAL_PENDING` | `PENDING`               | Policy decision was `REQUIRE_APPROVAL`; an `ApprovalRequest` is in `DRAFT` or `AWAITING_OPERATOR` (S5.3 §3.1).                                                      |
| `OVERRIDE_PENDING` | `PENDING`               | Policy decision was a non-`NonOverridableClass` hard-`DENY` and an `OverrideRequest` (S5.4 §3.1) is in `OS_REQUESTED` or `OS_AWAITING_DUAL_CONFIRM`.                |
| `APPROVED`         | `PENDING`               | A valid `ApprovalBinding` (S5.3 §5) or `OverrideBinding` (S5.4 §5) is held; not yet queued.                                                                         |
| `POLICY_DENIED`    | `FAILED`                | Terminal under normal flow. Set when Policy Kernel returned `DENY` (hard or scoped) and no override grant is in flight.                                             |
| `OVERRIDE_DENIED`  | `FAILED`                | Terminal. Set when the override path itself denied (e.g. `TARGET_NOT_OVERRIDABLE`, `INSUFFICIENT_QUORUM`, `TTL_EXPIRED` — see S5.4 §3.5).                           |
| `QUEUED`           | `PENDING`               | Approved/override-bound and in the dispatch queue under one of the closed `QueueClass` buckets (§3.5).                                                              |
| `EXECUTING`        | `RUNNING`               | The adapter has been dispatched under an `ActionDispatchKind` (§3.2) with the composed `SandboxProfile` (S3.2) applied.                                             |
| `VERIFYING`        | `RUNNING`               | Adapter returned success; `VerificationEngine.RunVerification` (S2.4 §11) is in flight against the envelope's `verification_intent`.                                |
| `SUCCEEDED`        | `SUCCEEDED`             | Terminal. Adapter executed and verification passed.                                                                                                                 |
| `FAILED`           | `FAILED`                | Terminal under non-rollback flow. Set when execution or verification failed and `rollback_strategy = NONE`, or when rollback was deliberately skipped.              |
| `ROLLED_BACK`      | `ROLLED_BACK`           | Terminal. Adapter rollback completed successfully after an execution or verification failure.                                                                       |
| `ROLLBACK_FAILED`  | `FAILED`                | Terminal forensic state. Adapter rollback was attempted but the rollback itself failed; system is in a degraded state and operator intervention is required (§7.4). |

The fourteen-state set is sealed. `OVERRIDE_PENDING` and `OVERRIDE_DENIED` are the constitutional seams between the normal-flow lifecycle and the S5.4 override path. `ROLLBACK_FAILED` is deliberately distinct from `FAILED`: it carries the additional invariant that a rollback was attempted and did not succeed, which has different operational consequences (operator alert, FOREVER evidence, no automatic retry). Conflating the two would erase a forensically significant distinction.

### §3.2 `ActionDispatchKind`

Closed enum. The runtime decides how the action is handed to its adapter. The decision is a function of the adapter manifest, the action's `Risk` flags (S0.1 §4.7), the policy decision's `Constraints.sandbox_profile_id` (S2.3 §10), and the action subject's `is_ai` flag.

| Value              | Semantics                                                                                                                                                              |
| ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `IN_PROCESS_RPC`   | Adapter handler runs inside the runtime's process. Reserved for low-latency, no-mutation adapters (e.g. status queries) declared `STABLE` and `TYPED_PARAMETERS_ONLY`. |
| `SUBPROCESS_FORK`  | Per-action subprocess; default for filesystem mutation, service control, package operations on host-bounded adapters.                                                  |
| `ISOLATED_SANDBOX` | Full sandbox per S3.2 `SandboxProfile`. Required for any AI-origin action (subject `is_ai = true`). Required for any action whose `risk.privileged` is true.           |
| `DRY_RUN`          | No mutation; adapter produces a simulation transcript only. Forced by `request.dry_run = SIMULATE` per S0.1 §9.3.                                                      |

The decision rule is a closed table:

```text
If request.dry_run == SIMULATE              -> DRY_RUN
Else if subject.is_ai == true               -> ISOLATED_SANDBOX
Else if request.risk.privileged == true     -> ISOLATED_SANDBOX
Else if adapter.manifest.dispatch_kind ==
        SUBPROCESS_FORK                     -> SUBPROCESS_FORK
Else if adapter.manifest.dispatch_kind ==
        IN_PROCESS_RPC
   AND adapter.manifest.stability == STABLE -> IN_PROCESS_RPC
Else                                        -> SUBPROCESS_FORK
```

`IN_PROCESS_RPC` requires explicit `STABLE` stability; `EXPERIMENTAL` and `DEPRECATED` adapters never run in-process regardless of manifest declaration. AI-origin actions always upgrade to `ISOLATED_SANDBOX`; an adapter manifest that declares `IN_PROCESS_RPC` for an action kind that AI subjects can request is allowed (the adapter is reusable across origins) but the runtime overrides at dispatch time.

### §3.3 `AdapterIOMode`

Closed enum. Free-form shell command input is **not** a value. The L3 layer invariant from `00_overview.md` (`Adapters must not accept free-form shell commands as primary input`) and INV-013 (AI cannot perform system admin operations) preclude it. An adapter that needs templated parameter substitution must declare `TEMPLATE_PARAMETERS` and use a closed substitution-variable vocabulary owned by the adapter manifest.

| Value                   | Semantics                                                                                                                                                                                                                                                                           |
| ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `TYPED_PARAMETERS_ONLY` | Adapter accepts `request.target` as a typed proto/JSON struct validated against the adapter manifest's per-action `target_schema`. Default mode.                                                                                                                                    |
| `TEMPLATE_PARAMETERS`   | Adapter accepts a typed template (also schema-validated) with closed substitution variables bound from `request.target`. Used for adapters that legitimately need to construct command lines (e.g. `pkg.install` shelling to `dnf`) without exposing free-form shell to the caller. |

The `TEMPLATE_PARAMETERS` mode does **not** relax the no-shell-injection rule. The template is a closed string with named placeholders; the adapter substitutes only those placeholders, in a quoting context the template author specified, against typed values from `request.target`. There is no way for a caller — human, AI, or otherwise — to inject a sub-command, a redirection, or any unbound shell metacharacter. An adapter manifest that declares a template with unbound `${...}` tokens fails registration with `ADAPTER_REGISTRATION_REJECTED`.

### §3.4 `AdapterStability`

Closed enum.

| Value          | Semantics                                                                                                                                                                                                                                                                      |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `REGISTERED`   | Adapter manifest accepted; not yet promoted past the initial registration barrier; treated as `EXPERIMENTAL` for dispatch purposes.                                                                                                                                            |
| `EXPERIMENTAL` | Functional but not yet hardened. AI-origin actions targeting `EXPERIMENTAL` adapters are forced to `DRY_RUN` by default; an explicit policy clearance is required for live execution. Receives extra audit (every action emits `EXPERIMENTAL_ADAPTER_LIVE_DISPATCH` evidence). |
| `STABLE`       | Hardened. Eligible for `IN_PROCESS_RPC` dispatch. Default for production adapters.                                                                                                                                                                                             |
| `DEPRECATED`   | Still accepted for execution but emits `ADAPTER_DEPRECATED_DISPATCH` on every call; new actions targeting it are discouraged by the runtime's tooling.                                                                                                                         |
| `RETIRED`      | No new dispatches accepted. `ListAdapters` still returns the adapter for forensic reasons. Action submissions targeting a `RETIRED` adapter fail with `UnknownAdapter`.                                                                                                        |

Stability is a property of the adapter, not of any individual action. Stability transitions are operator-only typed actions and themselves flow through the runtime (`runtime.adapter.set_stability`).

### §3.5 `QueueClass`

Closed enum. Each registered action is dispatched through exactly one queue class.

| Value               | Selection rule                                                                      | p95 queue wait | Notes                                                     |
| ------------------- | ----------------------------------------------------------------------------------- | -------------- | --------------------------------------------------------- |
| `INTERACTIVE`       | Operator-initiated (subject_type ∈ `human`) and `request.environment != AIR_GAPPED` | < 200 ms       | Highest priority outside recovery.                        |
| `AGENT_PROPOSAL`    | AI-initiated (subject `is_ai = true`)                                               | < 2 s          | Fairness-bounded; capped at 50 % of total queue capacity. |
| `BACKGROUND`        | Scheduled jobs, timers, application-internal cleanup actions                        | < 30 s         | Yielding to higher classes.                               |
| `RECOVERY_PRIORITY` | Any action while `host.recovery_mode = true`; preempts all other classes            | < 200 ms       | Recovery-mode only.                                       |

AI subjects attempting to submit on `INTERACTIVE` are silently downgraded to `AGENT_PROPOSAL` and an `AI_INTERACTIVE_QUEUE_DOWNGRADE` evidence record is emitted (§13). The downgrade is silent at the action level (no failure) but loud at the audit level (every downgrade is forensically visible).

### §3.6 `ExecutionFailureReason`

Closed enum, twelve values. Populated on transitions to `FAILED` or `ROLLED_BACK` to discriminate the failure cause. Mirrored into the S0.1 `Error.code` field where the corresponding canonical code exists.

| Value                             | When set                                                                                                   |
| --------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `SANDBOX_APPLICATION_FAILED`      | The composed `SandboxProfile` (S3.2 `ComposeProfile`) could not be applied at dispatch time.               |
| `ADAPTER_TIMEOUT`                 | The adapter exceeded its declared `adapter_timeout_seconds`.                                               |
| `ADAPTER_PANIC`                   | The adapter process exited with a non-zero status or panicked mid-execution.                               |
| `RESOURCE_BUDGET_EXCEEDED`        | The action's queue class budget, the per-subject rate limit, or the AI-share cap was exceeded at dispatch. |
| `DEPENDENCY_UNREADY`              | A declared adapter dependency (e.g. systemd, AIOS-FS) was not in a ready state at dispatch.                |
| `BACKEND_UNAVAILABLE`             | An external backend the adapter required (e.g. dnf metadata, AIOS-FS WAL) was unreachable.                 |
| `IDEMPOTENCY_KEY_REPLAY_DETECTED` | The `idempotency_key` was reused with a different `request_hash` (S0.1 §3.3).                              |
| `ENVELOPE_VALIDATION_FAILED`      | The envelope failed schema validation, target schema validation, or trace-context validation.              |
| `ROLLBACK_PRECONDITION_FAILED`    | A rollback was requested but the adapter's declared `rollback_precondition` was not met.                   |
| `BINDING_EXPIRED`                 | The held `ApprovalBinding` (S5.3) or `OverrideBinding` (S5.4) was expired or revoked at dispatch.          |
| `BINDING_VOIDED_ACTION_REVISED`   | The action's canonical hash at dispatch differs from the bound `bound_action_canonical_hash`.              |
| `ADAPTER_REFUSED`                 | The adapter ran but explicitly refused the action (e.g. precondition assertion).                           |

### §3.7 `RollbackOutcome`

Closed enum.

| Value            | Semantics                                                                                                          |
| ---------------- | ------------------------------------------------------------------------------------------------------------------ |
| `NOT_ATTEMPTED`  | Rollback was not attempted (e.g. action succeeded; or `rollback_strategy = NONE` on the adapter).                  |
| `SUCCEEDED`      | Adapter rollback returned success; lifecycle transitions to `ROLLED_BACK`.                                         |
| `FAILED`         | Adapter rollback returned failure or panicked; lifecycle transitions to `ROLLBACK_FAILED`; operator alert emitted. |
| `NOT_APPLICABLE` | The action was idempotent or read-only and rollback semantics do not apply (e.g. a query action).                  |

### §3.8 `RuntimeErrorCode`

Closed enum, twenty values, used in RPC-level error responses (gRPC status detail). Distinct from S0.1 `Error.code`: this enum carries L3-internal failures of the orchestration RPCs themselves, not failures of the actions they orchestrate.

| Value                             | Semantics                                                                                      |
| --------------------------------- | ---------------------------------------------------------------------------------------------- |
| `RUNTIME_OK`                      | No error. Reserved zero-value indicator.                                                       |
| `INVALID_ENVELOPE`                | The envelope failed pre-validation; details point to the offending field.                      |
| `UNKNOWN_ACTION_KIND`             | The `request.action` does not map to any registered adapter's declared `action_kinds`.         |
| `UNKNOWN_ADAPTER`                 | A direct adapter lookup by id failed (`ListAdapters` / `GetAdapterCapabilities`).              |
| `ADAPTER_NOT_DISPATCHABLE`        | The adapter exists but is in `RETIRED` stability or is `DEGRADED` past the dispatch threshold. |
| `POLICY_DECISION_UNAVAILABLE`     | The Policy Kernel was unreachable or returned an internal error.                               |
| `APPROVAL_BINDING_INVALID`        | The presented `ApprovalBinding` failed signature, scope, or hash check.                        |
| `OVERRIDE_BINDING_INVALID`        | The presented `OverrideBinding` failed signature, scope, or hash check.                        |
| `BINDING_HASH_MISMATCH`           | The action's canonical hash does not match the binding's `bound_action_canonical_hash`.        |
| `LIFECYCLE_ILLEGAL_TRANSITION`    | A request would drive the FSM through a transition not listed in §4.                           |
| `LIFECYCLE_TERMINAL`              | The action is in a terminal state; the requested operation is no longer valid.                 |
| `IDEMPOTENCY_REPLAY`              | Same `idempotency_key` with a different `request_hash` (S0.1 §3.3).                            |
| `QUEUE_BACKPRESSURE_REJECTED`     | Queue depth exceeded the health threshold and the runtime is shedding load.                    |
| `ADAPTER_TIMEOUT_BUDGET_EXCEEDED` | The adapter would not respect a budget the manifest authoritatively requires.                  |
| `VERIFICATION_GRAMMAR_REJECTED`   | The envelope's `verification_intent` failed S2.4 grammar validation at submission.             |
| `EVIDENCE_LOG_UNAVAILABLE`        | The evidence log refused an append; the runtime fails closed.                                  |
| `EVIDENCE_TAMPER_DETECTED`        | A `TAMPER_DETECTED` event from S3.1 is active; the runtime is in degraded mode.                |
| `RUNTIME_DEGRADED`                | The runtime itself is in degraded mode (e.g. clock rewind, adapter directory unloaded).        |
| `MANIFEST_SIGNATURE_INVALID`      | An adapter manifest registration failed signature verification.                                |
| `RUNTIME_INTERNAL`                | Catch-all for unexpected internal faults; details carry the trace id for forensic follow-up.   |

`RuntimeErrorCode` is mapped to gRPC status codes by the public ingress layer; the mapping is informational and does not relax the closed enum's semantics.

## §4 Lifecycle FSM (closed)

The fourteen states from §3.1 form a strictly closed FSM. Transitions not listed here are forbidden. An attempt to drive the FSM through an illegal transition is rejected with `RuntimeErrorCode = LIFECYCLE_ILLEGAL_TRANSITION` and the runtime emits an evidence record so the offence is forensically visible.

### §4.1 Diagram

```text
                                +-----------+
                                |  CREATED  |
                                +-----+-----+
                                      |
                                      v
                              +---------------+
                              | POLICY_PENDING|
                              +-------+-------+
                                      |
            policy=ALLOW              |     policy=REQUIRE_APPROVAL
            ----------------+         |         +---------------------+
                            |         |         |                     |
                            v         v         v                     v
                       +--------+  +-----+  +--------------+   +---------------+
                       | QUEUED |<-+APPRD+--|APPROVAL_PEND |   |OVERRIDE_PEND  |
                       +---+----+  +-----+  +------+-------+   +-------+-------+
                           |         ^             |                   |
                           v         |             v                   v
                      +---------+    |        +-----------+      +-----------+
                      |EXECUTING|    +--------+ APPROVED  +<-----+ APPROVED  |
                      +----+----+    (override)+-----+----+      +-----+-----+
                           |                          |                |
                           v                          (queue)          |
                      +---------+                     |                v
                      |VERIFYING|                     |        +-----------------+
                      +----+----+                     |        | OVERRIDE_DENIED |
                           |                          |        +-----------------+
              +------------+------------+             |
              |            |            |             |
              v            v            v             |
     +-----------+  +-----------+  +-----------+      |
     | SUCCEEDED |  |  FAILED   |  |ROLLED_BACK|      |
     +-----------+  +-----------+  +-----------+      |
                          |                            |
                          |                            |
                          v                            v
                  +-----------------+         +----------------+
                  | ROLLBACK_FAILED |         | POLICY_DENIED  |
                  +-----------------+         +----------------+
```

### §4.2 Allowed transitions, exhaustive

| #   | From               | To                 | Trigger                                                                                                        |
| --- | ------------------ | ------------------ | -------------------------------------------------------------------------------------------------------------- |
| T1  | (init)             | `CREATED`          | Envelope accepted by `ValidateAction`; pre-validation pending.                                                 |
| T2  | `CREATED`          | `POLICY_PENDING`   | Pre-validation succeeded; `EvaluatePolicyForAction` dispatched.                                                |
| T3  | `CREATED`          | `FAILED`           | Pre-validation failed (`INVALID_ENVELOPE`, `UNKNOWN_ACTION_KIND`, `IDEMPOTENCY_REPLAY`).                       |
| T4  | `POLICY_PENDING`   | `APPROVED`         | Policy decision = `ALLOW` (no approval required, no override path).                                            |
| T5  | `POLICY_PENDING`   | `APPROVAL_PENDING` | Policy decision = `REQUIRE_APPROVAL`; `ApprovalRequest` issued (S5.3).                                         |
| T6  | `POLICY_PENDING`   | `POLICY_DENIED`    | Policy decision = `DENY` (hard or scoped); no override path is available for the rule class.                   |
| T7  | `POLICY_PENDING`   | `OVERRIDE_PENDING` | Policy decision = `DENY` (scoped) and the operator has authored a non-`NonOverridableClass` `OverrideRequest`. |
| T8  | `APPROVAL_PENDING` | `APPROVED`         | `ApprovalBinding` issued (`AWAITING_OPERATOR → GRANTED` per S5.3 §6).                                          |
| T9  | `APPROVAL_PENDING` | `FAILED`           | `ApprovalRequest` terminated in any non-`GRANTED` state (DENIED, EXPIRED, REVOKED, FAILED_DELIVERY).           |
| T10 | `OVERRIDE_PENDING` | `APPROVED`         | `OverrideBinding` issued (`OS_REQUESTED/OS_AWAITING_DUAL_CONFIRM → OS_ACTIVE` per S5.4 §6).                    |
| T11 | `OVERRIDE_PENDING` | `OVERRIDE_DENIED`  | `OverrideRequest` terminated in any non-`OS_ACTIVE` state (`OS_DENIED`, `OS_EXPIRED`, `OS_REVOKED`).           |
| T12 | `APPROVED`         | `QUEUED`           | The action is enrolled in its `QueueClass` bucket.                                                             |
| T13 | `QUEUED`           | `EXECUTING`        | The eight-step pre-dispatch sequence (§6.1) succeeded; adapter dispatched under chosen `ActionDispatchKind`.   |
| T14 | `QUEUED`           | `FAILED`           | The eight-step pre-dispatch sequence failed at any step; `ExecutionFailureReason` populated.                   |
| T15 | `EXECUTING`        | `VERIFYING`        | Adapter returned success (or, for `DRY_RUN`, returned a simulation transcript).                                |
| T16 | `EXECUTING`        | `FAILED`           | Adapter returned failure, panicked, or timed out.                                                              |
| T17 | `VERIFYING`        | `SUCCEEDED`        | `VerificationEngine.RunVerification` returned `VERIFICATION_PASSED` for all intents.                           |
| T18 | `VERIFYING`        | `FAILED`           | Verification returned `FAILED`, `TIMEOUT`, `PROBE_ERROR`, or `SKIPPED` and the rollback strategy is `NONE`.    |
| T19 | `FAILED`           | `ROLLED_BACK`      | Rollback strategy != `NONE`; adapter `Rollback` returned `RollbackOutcome.SUCCEEDED`.                          |
| T20 | `FAILED`           | `ROLLBACK_FAILED`  | Rollback strategy != `NONE`; adapter `Rollback` returned `RollbackOutcome.FAILED` (terminal).                  |
| T21 | `POLICY_DENIED`    | `OVERRIDE_PENDING` | Operator authored a non-`NonOverridableClass` `OverrideRequest` after the original `POLICY_DENIED` transition. |

Forbidden transitions (every attempted forbidden transition is itself a `LIFECYCLE_ILLEGAL_TRANSITION` event and emits evidence):

- Any transition out of `SUCCEEDED`, `ROLLED_BACK`, or `ROLLBACK_FAILED` (terminal).
- Any transition out of `OVERRIDE_DENIED` (terminal).
- `EXECUTING → SUCCEEDED` directly without `VERIFYING` (verification is mandatory after adapter success).
- `QUEUED → EXECUTING` without successful pre-dispatch (the eight steps are not optional).
- `APPROVED → EXECUTING` directly without `QUEUED` (queue enrolment is the single concurrency-control gate).
- `APPROVAL_PENDING → APPROVED` from any state other than the `ApprovalBinding GRANTED` event.
- `OVERRIDE_PENDING → APPROVED` from any state other than the `OverrideBinding OS_ACTIVE` event.

### §4.3 State persistence

Lifecycle state persists in the runtime's authoritative AIOS-FS object (per S1.3 transactional semantics). Crash recovery preserves in-flight states: an action in `QUEUED` at crash time resumes in `QUEUED` at boot; an action in `EXECUTING` at crash time resumes in a fresh `FAILED` with `ExecutionFailureReason = ADAPTER_PANIC` and `crash_recovery_observed = true` because the runtime cannot prove the adapter completed cleanly. The conservative posture is constitutional — better an action observed as `FAILED` and re-proposed than an action observed as `SUCCEEDED` without verification evidence.

### §4.4 Single-writer principle

A single L3 instance owns the lifecycle of any one action. The instance id is recorded as `capability_runtime_id` on the envelope (S0.1 §5.1). Other instances may read the lifecycle through the public ingress but must not write. Multi-host federation is out of scope.

## §5 gRPC service surface (closed)

The S10.1 surface is the closed `aios.runtime.v1alpha1.CapabilityRuntime` service. Nine RPCs cover validation, policy orchestration, approval orchestration, dispatch, verification, rollback, status, adapter discovery, and runtime info.

### §5.1 Service definition

```proto
syntax = "proto3";
package aios.runtime.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/struct.proto";
import "google/protobuf/empty.proto";

// All RPCs operate over the S0.1 envelope; the L3 internal lifecycle is exposed
// here in addition to the S0.1 phase projection.

service CapabilityRuntime {
  // Pre-validation: schema, target, sandbox-profile validity, dry-run feasibility.
  rpc ValidateAction(ValidateActionRequest) returns (ValidateActionResponse);

  // Orchestrates the L4.1 EvaluatePolicy call with action-bound enrichment;
  // returns the policy decision plus the L3 lifecycle state observed.
  rpc EvaluatePolicyForAction(EvaluatePolicyForActionRequest)
      returns (EvaluatePolicyForActionResponse);

  // Orchestrates the L4.3 approval flow when policy outcome was REQUIRE_APPROVAL.
  rpc RequestApprovalForAction(RequestApprovalForActionRequest)
      returns (RequestApprovalForActionResponse);

  // Terminal dispatcher; consumes a held binding (approval or override),
  // runs the eight-step pre-dispatch (§6.1), and dispatches the adapter.
  rpc ExecuteAction(ExecuteActionRequest) returns (ExecuteActionResponse);

  // Runs S2.4 VerificationEngine.RunVerification against the envelope's
  // verification_intent and projects the result into the lifecycle.
  rpc VerifyAction(VerifyActionRequest) returns (VerifyActionResponse);

  // Adapter-driven rollback per the action's declared rollback_strategy.
  rpc RollbackAction(RollbackActionRequest) returns (RollbackActionResponse);

  // Read-only snapshot of an action's L3-internal lifecycle and its envelope.
  rpc GetActionStatus(GetActionStatusRequest) returns (GetActionStatusResponse);

  // Adapter directory.
  rpc ListAdapters(ListAdaptersRequest) returns (ListAdaptersResponse);

  // What an adapter advertises (typed action kinds, schemas, stability).
  rpc GetAdapterCapabilities(GetAdapterCapabilitiesRequest)
      returns (GetAdapterCapabilitiesResponse);

  // Runtime self-description (version, configured adapters, queue depth, health).
  rpc GetCapabilityRuntimeInfo(google.protobuf.Empty)
      returns (GetCapabilityRuntimeInfoResponse);
}
```

The service is the L3-internal authority. The S0.1 public ingress (`aios.action.v1alpha1.CapabilityRuntime.SubmitAction`) calls into this surface; external callers do not need to know about the internal RPCs but they may inspect the lifecycle through `GetActionStatus`.

### §5.2 Message shapes

#### `ValidateAction`

```proto
message ValidateActionRequest {
  // The full S0.1 envelope. Identity and Request sections are caller-immutable;
  // Execution and Trace are populated by the runtime and may be empty here.
  aios.action.v1alpha1.ActionEnvelope envelope = 1;
}

message ValidateActionResponse {
  ActionLifecycleState state = 1;        // typically CREATED on success, FAILED on rejection
  RuntimeErrorCode error = 2;            // RUNTIME_OK on success
  repeated ValidationFinding findings = 3;
  string action_request_id = 4;          // L3-internal handle, "act:" + ULID
}

message ValidationFinding {
  string field_path = 1;                 // dotted path into the envelope
  string code = 2;                       // canonical short code, e.g. "TargetSchemaInvalid"
  string message = 3;                    // English plain-text
  bool is_blocking = 4;                  // true for any finding that prevented acceptance
}
```

`ValidateAction` runs the following checks, in order, fail-closed:

1. Envelope schema validation against `aios.action.v1alpha1` (per S0.1).
2. `request.action` ↔ adapter manifest lookup (UNKNOWN_ACTION_KIND if absent).
3. `request.target` ↔ adapter manifest's per-action `target_schema`.
4. `request.verification` ↔ S2.4 grammar (each intent's typed args validated).
5. `request.sandbox_profile_id` ↔ S3.2 profile registry (must exist or be empty).
6. `idempotency_key` collision check (`IDEMPOTENCY_REPLAY` if hash drift).

If all checks pass, the runtime allocates an `action_request_id`, persists the envelope at `CREATED`, and returns. The action is now visible to `GetActionStatus`.

#### `EvaluatePolicyForAction`

```proto
message EvaluatePolicyForActionRequest {
  string action_request_id = 1;
  aios.action.v1alpha1.ActionEnvelope envelope = 2;
}

message EvaluatePolicyForActionResponse {
  ActionLifecycleState state = 1;        // POLICY_PENDING in flight, then APPROVED / APPROVAL_PENDING / OVERRIDE_PENDING / POLICY_DENIED on completion
  aios.policy.v1alpha1.PolicyDecision decision = 2;
  RuntimeErrorCode error = 3;
}
```

`EvaluatePolicyForAction` is a thin orchestration wrapper around `aios.policy.v1alpha1.PolicyKernel.EvaluatePolicy`. It:

1. Transitions the lifecycle from `CREATED` to `POLICY_PENDING`.
2. Calls `PolicyKernel.EvaluatePolicy` with the envelope and a snapshot identifier for enrichment determinism.
3. Records the returned `PolicyDecision` against the action.
4. Transitions the lifecycle to `APPROVED` (decision = `ALLOW`), `APPROVAL_PENDING` (`REQUIRE_APPROVAL`), `POLICY_DENIED` (`DENY` with no scoped-override path), or remains in `POLICY_PENDING` while a downstream override request is being authored.

The runtime never re-interprets the policy decision; it consumes the `Decision`, `Constraints`, and `ApprovalRequirement` verbatim. The decision is bound to the action's request hash (S2.3 §13) and re-checked at `ExecuteAction` against the active bundle version (§6.1).

#### `RequestApprovalForAction`

```proto
message RequestApprovalForActionRequest {
  string action_request_id = 1;
  string policy_decision_id = 2;
  aios.approval.v1alpha1.ApprovalRequirement requirement = 3;
}

message RequestApprovalForActionResponse {
  ActionLifecycleState state = 1;        // APPROVAL_PENDING, then APPROVED on grant or FAILED on terminal denial
  string approval_request_id = 2;        // S5.3 ApprovalRequest.approval_request_id
  RuntimeErrorCode error = 3;
}
```

`RequestApprovalForAction` orchestrates the S5.3 approval flow:

1. Constructs the `ApprovalRequest` from the policy decision's `ApprovalRequirement` plus the action's identity.
2. Submits the request to the L4.3 Approval Mechanics service.
3. Subscribes to the request's FSM (`AWAITING_OPERATOR → GRANTED | DENIED | EXPIRED | REVOKED | FAILED_DELIVERY`).
4. Transitions the action lifecycle to `APPROVED` on `GRANTED`, or to `FAILED` with the relevant `ExecutionFailureReason` on any terminal-denial state.

The runtime does not author the prompt UI tree; that is L4.3 + L7. The runtime does not select the channel; that is L4.3 §7. The runtime does not verify operator session class; that is L4.3 + L4 identity. This RPC is a binding between the action lifecycle and the approval lifecycle, nothing more.

#### `ExecuteAction`

```proto
message ExecuteActionRequest {
  string action_request_id = 1;
  // Either an approval binding or an override binding must be present unless
  // the policy decision was unconditional ALLOW (no approval required).
  aios.approval.v1alpha1.ApprovalBinding approval_binding = 2;
  aios.override.v1alpha1.OverrideBinding override_binding = 3;
}

message ExecuteActionResponse {
  ActionLifecycleState state = 1;        // EXECUTING in flight, then VERIFYING / FAILED on adapter return
  string adapter_id = 2;
  ActionDispatchKind dispatch_kind = 3;
  string applied_sandbox_profile_id = 4;  // S3.2 profile id actually applied
  RuntimeErrorCode error = 5;
  ExecutionFailureReason failure_reason = 6;  // populated on FAILED
}
```

`ExecuteAction` runs the strictly ordered eight-step pre-dispatch sequence (§6.1) and then dispatches the adapter under the chosen `ActionDispatchKind`. The eight steps are not optional; failure at any step terminates the action with `FAILED` and an appropriate `ExecutionFailureReason`. Detailed semantics are in §6.

#### `VerifyAction`

```proto
message VerifyActionRequest {
  string action_request_id = 1;
}

message VerifyActionResponse {
  ActionLifecycleState state = 1;        // VERIFYING in flight, then SUCCEEDED / FAILED
  repeated aios.action.v1alpha1.VerificationResult results = 2;
  RuntimeErrorCode error = 3;
}
```

`VerifyAction` is invoked automatically by `ExecuteAction` after adapter success; it is also exposed as an explicit RPC so the L9 verification subsystem can re-run verification against an action that completed but whose verification was deferred (e.g. an asynchronous probe). The implementation calls `aios.verify.v1alpha1.VerificationEngine.RunVerification` (S2.4 §11) once per `VerificationIntent` in the envelope, collects the results, and projects them into the action's `verification_results` (S0.1 §5.1).

#### `RollbackAction`

```proto
message RollbackActionRequest {
  string action_request_id = 1;
}

message RollbackActionResponse {
  ActionLifecycleState state = 1;        // ROLLED_BACK or ROLLBACK_FAILED on completion
  RollbackOutcome outcome = 2;
  RuntimeErrorCode error = 3;
}
```

`RollbackAction` consults the adapter manifest's per-action `rollback_strategy` (§7) and dispatches the adapter's `Rollback` handler if the strategy is not `NONE`. Outcomes per §3.7. `ROLLBACK_FAILED` is a terminal forensic state; it emits FOREVER evidence and triggers an operator alert.

#### `GetActionStatus`

```proto
message GetActionStatusRequest {
  string action_request_id = 1;
}

message GetActionStatusResponse {
  aios.action.v1alpha1.ActionEnvelope envelope = 1;  // current envelope (with execution.* populated)
  ActionLifecycleState lifecycle_state = 2;          // L3-internal fourteen-state lifecycle
  string current_adapter_id = 3;
  ActionDispatchKind current_dispatch_kind = 4;
  string applied_sandbox_profile_id = 5;
  google.protobuf.Timestamp last_state_change_at = 6;
}
```

Read-only snapshot. `NOT_FOUND` if the `action_request_id` is unknown. Unlike S0.1 `GetAction`, this RPC exposes the fourteen-state lifecycle directly.

#### `ListAdapters`

```proto
message ListAdaptersRequest {
  string action_kind_filter = 1;     // optional dotted prefix, e.g. "service.*"
  AdapterStability stability_filter = 2;  // optional; if UNSPECIFIED, all stabilities returned
  bool include_retired = 3;
}

message ListAdaptersResponse {
  repeated AdapterDirectoryEntry entries = 1;
}

message AdapterDirectoryEntry {
  string adapter_id = 1;
  AdapterStability stability = 2;
  AdapterDispatchKind dispatch_kind = 3;
  AdapterIOMode io_mode = 4;
  repeated string declared_action_kinds = 5;
  AdapterHealth health = 6;
  google.protobuf.Timestamp registered_at = 7;
  google.protobuf.Timestamp last_seen_at = 8;
  string manifest_signature_status = 9;  // "VALID", "EXPIRED", "REVOKED"
}

enum AdapterHealth {
  ADAPTER_HEALTH_UNSPECIFIED = 0;
  ADAPTER_HEALTHY            = 1;
  ADAPTER_DEGRADED           = 2;
  ADAPTER_UNHEALTHY          = 3;
}
```

#### `GetAdapterCapabilities`

```proto
message GetAdapterCapabilitiesRequest {
  string adapter_id = 1;
}

message GetAdapterCapabilitiesResponse {
  AdapterManifest manifest = 1;
  repeated AdapterActionCapability capabilities = 2;
}

message AdapterActionCapability {
  string action_kind = 1;             // dotted name from manifest
  google.protobuf.Struct target_schema = 2;
  string rollback_strategy = 3;       // see §7
  uint32 default_timeout_seconds = 4;
}
```

#### `GetCapabilityRuntimeInfo`

```proto
message GetCapabilityRuntimeInfoResponse {
  string capability_runtime_id = 1;
  string runtime_version = 2;
  repeated string supported_schema_versions = 3;
  uint32 registered_adapters_count = 4;
  uint32 queue_depth = 5;
  bool degraded = 6;
  string degraded_reason = 7;        // empty when degraded = false
  google.protobuf.Timestamp started_at = 8;
}
```

## §6 Execute path (eight-step pre-dispatch)

`ExecuteAction` is the constitutional gate between "approved" and "running". The eight steps below are strictly ordered, fail-closed, and exhaustive. The runtime never skips a step under any circumstance — including recovery mode. INV-014 (no proof, no completion) is enforced here: the runtime cannot claim execution without first proving the action is still valid against the live state of the system.

### §6.1 The eight steps

1. **Re-validate canonical hash.** Recompute `hex_lower(BLAKE3(JCS(envelope.request)))[:32]`. Compare against the binding's `bound_action_canonical_hash` (S5.3 §13.2 / S5.4 §5). Mismatch → `BINDING_VOIDED_ACTION_REVISED`, FOREVER evidence, transition `FAILED`.
2. **Re-evaluate policy decision against current bundle version.** Read the active policy bundle's `bundle_version` from L4.1. If it differs from the `bundle_version` recorded on the policy decision, re-run `PolicyKernel.EvaluatePolicy` against the current bundle. If the new decision is no longer `ALLOW` (e.g. the operator rolled back to a stricter bundle), transition `FAILED` with `ExecutionFailureReason = BINDING_EXPIRED`.
3. **Re-check approval binding state.** For approval-bound actions: query L4.3 for the binding's current state. Must be `GRANTED`. If `EXPIRED`, `REVOKED`, `CONSUMED`, or `DENIED`, transition `FAILED` with `BINDING_EXPIRED`. For override-bound actions: query L4.5 for `OS_ACTIVE`. Equivalent fail-closed behaviour.
4. **Re-check capability binding state via the Vault Broker.** If the action's policy decision attached `Constraints.vault_capability_required` (S2.3 §10), re-check the capability with the L4.2 Vault Broker. The broker performs its operations without revealing material; this step confirms the capability has not been revoked since policy evaluation. Failure → `FAILED` with `BINDING_EXPIRED`.
5. **Re-evaluate sandbox profile composition.** Call `aios.sandbox.v1alpha1.SandboxComposer.ComposeProfile` (S3.2 §3) using the current host capability snapshot. The composed profile may differ from the one recorded at policy evaluation if backends changed (e.g. landlock unavailable after a kernel upgrade). If composition fails (no compatible profile possible under the floor), transition `FAILED` with `SANDBOX_APPLICATION_FAILED`.
6. **Re-check action lifecycle is in `QUEUED`.** A concurrent `RollbackAction` or operator cancellation may have transitioned the action out of `QUEUED` between dequeue and dispatch. If the lifecycle is no longer `QUEUED`, abort with `RuntimeErrorCode = LIFECYCLE_ILLEGAL_TRANSITION`.
7. **Mark the binding `CONSUMED`.** Atomically transition the held `ApprovalBinding` to `CONSUMED` (S5.3 §13.1) or the `OverrideBinding` to `OS_CONSUMED` (S5.4 §6). This is the constitutional anti-replay gate: a second `ExecuteAction` against the same binding will observe a terminal state and fail closed.
8. **Transition lifecycle to `EXECUTING` and dispatch the adapter.** The adapter is invoked under the chosen `ActionDispatchKind` (per §3.2's decision rule), inside the composed `SandboxProfile`, with the typed `request.target` (or templated parameters per `AdapterIOMode = TEMPLATE_PARAMETERS`).

If any step fails, the runtime emits an `EXECUTION_FAILED` evidence record with the relevant `ExecutionFailureReason` and transitions the lifecycle to `FAILED`. There is no partial dispatch and no silent recovery.

### §6.2 Dispatch envelope

The adapter is invoked with a typed `AdapterDispatchEnvelope` that carries:

```proto
message AdapterDispatchEnvelope {
  string action_request_id = 1;
  string action_kind = 2;
  google.protobuf.Struct target = 3;             // schema-validated by §6.1 step 0 (ValidateAction)
  string applied_sandbox_profile_id = 4;
  ActionDispatchKind dispatch_kind = 5;
  bool dry_run = 6;                              // forced true when dispatch_kind = DRY_RUN
  uint32 adapter_timeout_seconds = 7;            // bounded by manifest default + policy override
  string trace_id = 8;
  string vault_capability_handle = 9;            // opaque handle from §6.1 step 4; never raw secret material
}
```

The adapter never receives the raw approval binding, the override binding, the policy decision body, or any vault material. It receives a typed request with bounded scope plus a sandbox profile already applied. This is the structural manifestation of INV-002 (AI proposes never executes): the adapter is the executor and is intentionally informationally narrowed.

### §6.3 Adapter response

The adapter returns:

```proto
message AdapterDispatchResponse {
  AdapterStatus status = 1;
  google.protobuf.Struct output = 2;             // schema-validated by manifest's response_schema
  string raw_output_ref = 3;                     // optional; pointer to large output, redacted at projection
  bool changed = 4;                              // did the adapter mutate observable state?
  google.protobuf.Struct rollback_handle = 5;    // opaque, given back to Rollback() if needed
  string adapter_failure_message = 6;            // populated on FAILED; secret-redacted
}

enum AdapterStatus {
  ADAPTER_STATUS_UNSPECIFIED = 0;
  ADAPTER_OK                  = 1;
  ADAPTER_FAILED              = 2;
  ADAPTER_TIMEOUT             = 3;
  ADAPTER_REFUSED             = 4;               // adapter explicitly refused (precondition fail)
  ADAPTER_PANIC               = 5;
}
```

`ADAPTER_OK` triggers transition to `VERIFYING`. The other four trigger transition to `FAILED` with the matching `ExecutionFailureReason`.

## §7 Verification and rollback

### §7.1 Verification

After `ADAPTER_OK`, the runtime transitions to `VERIFYING` and calls `VerifyAction` internally. The verification engine (S2.4) runs each `VerificationIntent` from the envelope. Outcomes:

| Verification status                     | Lifecycle transition | Evidence record                 |
| --------------------------------------- | -------------------- | ------------------------------- |
| `VERIFICATION_PASSED` (all intents)     | `SUCCEEDED`          | `EXECUTION_SUCCEEDED`           |
| `VERIFICATION_FAILED` (any intent)      | `FAILED`             | `EXECUTION_VERIFICATION_FAILED` |
| `VERIFICATION_TIMEOUT` (any intent)     | `FAILED`             | `EXECUTION_VERIFICATION_FAILED` |
| `VERIFICATION_PROBE_ERROR` (any intent) | `FAILED`             | `EXECUTION_VERIFICATION_FAILED` |
| `VERIFICATION_SKIPPED` (any intent)     | `FAILED`             | `EXECUTION_VERIFICATION_FAILED` |

`VERIFICATION_SKIPPED` is treated as a failure outside `VALIDATE` mode: an action that ran live without verification is by construction an unproven action, and INV-014 forbids unproven completion. `VALIDATE` mode (S0.1 §9.3) is the only context where `SKIPPED` is acceptable; in `VALIDATE` mode the action did not execute anyway.

If verification fails and the adapter declared a rollback strategy, the runtime proceeds to §7.2.

### §7.2 Rollback FSM

Rollback is closed and adapter-driven. Each adapter declares per-action-kind:

```proto
enum RollbackStrategy {
  ROLLBACK_STRATEGY_UNSPECIFIED = 0;
  NONE                          = 1;  // action is destructive without rollback path; never auto-rolled back
  IDEMPOTENT_REVERSE            = 2;  // adapter can compute the reverse action from the action and adapter state
  CHECKPOINT_BASED              = 3;  // adapter took a checkpoint pre-execute and can restore it
  EXTERNAL_REQUIRED             = 4;  // rollback requires operator intervention (e.g. restore from backup)
}
```

After a `FAILED` transition for an action whose `rollback_strategy != NONE`, the runtime calls the adapter's `Rollback(action_request_id, rollback_handle)`:

| Adapter Rollback() return        | Lifecycle transition | Evidence record                            | Operator alert |
| -------------------------------- | -------------------- | ------------------------------------------ | -------------- |
| `RollbackOutcome.SUCCEEDED`      | `ROLLED_BACK`        | `ROLLBACK_SUCCEEDED`                       | No             |
| `RollbackOutcome.FAILED`         | `ROLLBACK_FAILED`    | `ROLLBACK_FAILED_REQUIRES_OPERATOR`        | Yes (FOREVER)  |
| `RollbackOutcome.NOT_APPLICABLE` | `FAILED` (stays)     | `ROLLBACK_ATTEMPTED` (note=NOT_APPLICABLE) | No             |
| `RollbackOutcome.NOT_ATTEMPTED`  | `FAILED` (stays)     | `ROLLBACK_ATTEMPTED` (note=NOT_ATTEMPTED)  | No             |

`EXTERNAL_REQUIRED` is treated as `NOT_APPLICABLE` from the FSM's perspective — the runtime cannot roll back, the operator must. The `ROLLBACK_FAILED_REQUIRES_OPERATOR` record carries an `affected_resources` list so the operator knows what to inspect manually.

### §7.3 Rollback preconditions

If the adapter declares `rollback_strategy != NONE` but its precondition is not met at rollback time (e.g. the checkpoint was garbage-collected because an unrelated action consumed disk), the rollback fails with `ROLLBACK_PRECONDITION_FAILED`. This is the same as `RollbackOutcome.FAILED` from the FSM's perspective and lands in `ROLLBACK_FAILED`.

### §7.4 `ROLLBACK_FAILED` is terminal

There is no auto-retry for a failed rollback. The reasoning is operational: a rollback failure means the system has executed a partial state change and a partial reversal — an unknown intermediate state. Re-running rollback against that state would compound the unknown. The constitutional posture is: stop, alert, let an operator inspect.

`ROLLBACK_FAILED` emits `ROLLBACK_FAILED_REQUIRES_OPERATOR` at FOREVER retention, raises a high-priority operator alert through L9.4 admin operations, and the affected adapter is marked `ADAPTER_DEGRADED` (subsequent dispatches against it queue behind the degradation backpressure budget).

## §8 Hard-deny escape via emergency override

A scoped (non-`NonOverridableClass`) hard-deny may be relaxed by the S5.4 emergency override path. The L3 lifecycle integrates this path through the `OVERRIDE_PENDING` and `OVERRIDE_DENIED` states.

### §8.1 Path

```text
POLICY_PENDING --(scoped DENY)--> POLICY_DENIED
POLICY_DENIED  --(operator authors OverrideRequest)--> OVERRIDE_PENDING
OVERRIDE_PENDING --(OverrideBinding issued)--> APPROVED --> QUEUED --> EXECUTING --> ...
OVERRIDE_PENDING --(override denied / expired / revoked)--> OVERRIDE_DENIED (terminal)
```

`POLICY_DENIED → OVERRIDE_PENDING` (T21 in §4.2) is the seam. The transition is gated on:

- The `target_hard_deny_rule_id` is **not** in `NonOverridableClass` (S5.4 §10). Hard-constitutional denials cannot be relaxed.
- The override request has `target_action_canonical_hash` matching the action's canonical hash (S5.4 §4).
- The runtime is not in degraded mode for evidence (`EVIDENCE_TAMPER_DETECTED`).

The eight-step pre-dispatch in §6.1 is run identically for override-bound actions. Step 1 (canonical hash check), step 3 (binding state check, against `OverrideBinding` instead of `ApprovalBinding`), and step 7 (mark consumed, transitioning `OS_ACTIVE → OS_CONSUMED`) are constitutional anti-replay gates.

### §8.2 What override does not relax

Override does **not** bypass:

- The eight-step pre-dispatch sequence — every step still runs.
- Verification — an override-bound action that fails verification still transitions to `FAILED`, and rollback is attempted per §7.2.
- Evidence — every override-bound action emits the standard evidence record set, plus the FOREVER `OVERRIDE_CONSUMED` record from S5.4 §13.
- The sandbox floor — INV-002 still applies; AI-origin override-bound actions still dispatch under `ISOLATED_SANDBOX`.

Override changes the answer to "may this action proceed?", not to "what happens after it proceeds?".

## §9 Dry-run mode

S0.1 envelopes carry `dry_run: DryRunMode`. When the value is `SIMULATE`, the L3 lifecycle behaves as follows:

1. `ActionDispatchKind` is forced to `DRY_RUN` regardless of the manifest's preference (§3.2 decision rule).
2. The adapter is invoked with `dry_run = true` in the dispatch envelope.
3. The adapter must produce a simulation transcript in `output`. If the adapter does not declare `simulate` in its capabilities, `ExecuteAction` fails closed with `AdapterDoesNotSupportSimulate` (S0.1 §7.3 canonical code) and transitions `FAILED`.
4. No real-world side effects are applied. The sandbox is still set up (per S0.1 §9.4 — sandbox is applied for real because compromised adapters must not escape "because it's simulation").
5. Verification still runs against the adapter's simulation transcript. Verification probes are read-only by S2.4 §6.1, so they may be safely run against simulated state.
6. Evidence records are emitted with `kind = SIMULATION` (per the queued S3.1 RecordType `DRY_RUN_SIMULATION_RECORDED`). Production audit views filter the simulated stream by default.

The lifecycle still walks `CREATED → POLICY_PENDING → APPROVED → QUEUED → EXECUTING → VERIFYING → SUCCEEDED` (or `FAILED` on simulation failure). Approvals are honoured: a `SIMULATE` action whose policy outcome is `REQUIRE_APPROVAL` still requires a real approval (it does not auto-approve), but the binding's consumption marks `CONSUMED` against a simulated execution. Operator semantics: an approval granted for a `SIMULATE` action is not transferable to a `LIVE` action; the canonical hashes differ because `request.dry_run` is part of the canonical input.

`VALIDATE` mode (S0.1 §9.3) is the other dry-run flavour. It short-circuits before policy: `CREATED → SUCCEEDED` (or `FAILED` on validation error). No adapter is dispatched. No evidence is written by default; the caller may opt in via a future extension flag.

## §10 Adapter manifest contract

Each adapter is registered through a typed `AdapterManifest`. Registration is itself a typed action (`runtime.adapter.register`) that flows through the runtime. Self-registration is not allowed; the registration must be authored by an operator-class subject and signed.

### §10.1 Manifest schema

```proto
message AdapterManifest {
  string adapter_id = 1;                     // "adapter:<vendor>:<name>:<version>"
  string adapter_version = 2;                // SemVer; advisory
  string vendor = 3;                         // free-form; echoed in adapter_id
  string name = 4;                           // free-form; echoed in adapter_id

  AdapterStability declared_stability = 5;
  AdapterIOMode io_mode = 6;
  ActionDispatchKind dispatch_kind = 7;      // adapter's preferred kind; runtime may override per §3.2

  repeated AdapterActionDeclaration declared_actions = 8;
  repeated string declared_invariants_supported = 9;  // closed list of L0 INV-XXX ids the adapter respects

  uint32 default_adapter_timeout_seconds = 10;  // bounded by §15
  string default_sandbox_profile_id = 11;       // S3.2 profile id; runtime may compose to stricter

  string adapter_signature = 12;             // Ed25519 over JCS of fields 1..11; hex_lower
  string signing_key_id = 13;                // identity service or recognised publisher key id
  google.protobuf.Timestamp manifest_created_at = 14;
  google.protobuf.Timestamp manifest_expires_at = 15;
}

message AdapterActionDeclaration {
  string action_kind = 1;                    // dotted name from L5 capability catalog
  google.protobuf.Struct target_schema = 2;
  google.protobuf.Struct response_schema = 3;
  RollbackStrategy rollback_strategy = 4;
  uint32 timeout_seconds = 5;                // overrides manifest default for this action_kind
  string template_string = 6;                // populated only when io_mode = TEMPLATE_PARAMETERS
  repeated string template_substitution_variables = 7;  // closed list of allowed variables
}
```

### §10.2 Signature discipline

The manifest is signed by an Ed25519 key recognised by the AIOS root or a recognised publisher (the trust chain mirrors S2.3 §12.3 policy bundle trust):

```text
AIOS root key  ──signs──▶  Publisher key  ──signs──▶  Adapter manifest
```

Verification at registration:

1. Manifest signature must verify against the publisher key in the AIOS trust store.
2. Publisher must be endorsed for the adapter's domain (e.g. `service.*` adapters require service-domain endorsement).
3. The manifest's `manifest_expires_at` must be in the future.
4. Each `declared_action_kind` must exist in the L5 capability catalog (S1.1 §6.4); an adapter cannot register an action kind that has no public catalog entry.

Failure at any step → registration rejected with `MANIFEST_SIGNATURE_INVALID` and a FOREVER `ADAPTER_REGISTRATION_REJECTED` evidence record.

### §10.3 Stability promotion

Stability is independent of the manifest's `declared_stability` field. The runtime treats `declared_stability` as the maximum the adapter may claim; the actual operating stability is set by an operator through `runtime.adapter.set_stability` (a typed action). Stability transitions are evidence-logged.

### §10.4 Manifest expiry and rotation

`manifest_expires_at` is mandatory. An adapter whose manifest expires is automatically de-registered with `ADAPTER_DEREGISTERED` (`reason = MANIFEST_EXPIRED`). Operators rotate the manifest by submitting a new `runtime.adapter.register` with the same `adapter_id` and a fresh signature; in-flight actions on the old manifest finish on the old manifest, new dispatches use the new manifest.

### §10.5 Action-kind exclusivity

An action kind is declared by exactly one adapter. The first manifest to register an action kind owns it. A second registration attempting to declare the same action kind is rejected with `ADAPTER_KIND_COLLISION` (a fresh `RuntimeErrorCode` reserved value, see §3.8 — `ADAPTER_NOT_DISPATCHABLE` covers the runtime-side observation). The reasoning is that two adapters claiming the same kind would force the runtime to make a routing decision the manifest contract is designed to avoid.

## §11 Queueing, fairness, AI-share cap

### §11.1 Per-class capacity

The runtime maintains four queue classes (§3.5). Their relative capacity is configurable by the operator but bounded by constitutional defaults:

| Queue               | Default share of total capacity | Max share               | Min share |
| ------------------- | ------------------------------- | ----------------------- | --------- |
| `INTERACTIVE`       | 30 %                            | 60 %                    | 20 %      |
| `AGENT_PROPOSAL`    | 40 % (cap)                      | 50 %                    | 20 %      |
| `BACKGROUND`        | 25 %                            | 50 %                    | 10 %      |
| `RECOVERY_PRIORITY` | 5 % (always reserved)           | 100 % (during recovery) | 5 %       |

`AGENT_PROPOSAL` is hard-capped at 50 % regardless of operator configuration. The cap is constitutional — an AI-saturated runtime is the operational equivalent of a hostile takeover, and the queue contract refuses to permit it.

### §11.2 Per-subject rate limits

Per-subject token-bucket rate limits prevent any one subject from monopolising a queue class:

| Subject type      | Default rate    | Burst |
| ----------------- | --------------- | ----- |
| Human             | 60 actions/min  | 30    |
| AI agent          | 30 actions/min  | 15    |
| Application       | 120 actions/min | 60    |
| Service           | 600 actions/min | 300   |
| Recovery operator | 240 actions/min | 120   |

Rate-limit denials emit `RESOURCE_BUDGET_EXCEEDED` evidence and transition the action to `FAILED`. The ratios are advisory; operator policy may tighten them but cannot disable them.

### §11.3 Backpressure

When total queue depth exceeds the health threshold (default 1 000 in-flight actions), the runtime enters backpressure mode:

- New `ExecuteAction` calls are rejected with `RuntimeErrorCode = QUEUE_BACKPRESSURE_REJECTED`.
- `INTERACTIVE` and `RECOVERY_PRIORITY` continue to dispatch; other classes are paused.
- An `ADAPTER_DEGRADED` event is **not** emitted unless the cause is adapter-side; backpressure is queue-side.
- The `runtime_degraded` gauge (§14) reflects backpressure mode.

### §11.4 AI interactive-queue downgrade

An AI subject that submits an action with `subject_type ∈ {agent, application}` and that would otherwise queue on `INTERACTIVE` (no other class fits) is silently downgraded to `AGENT_PROPOSAL`. The downgrade emits `AI_INTERACTIVE_QUEUE_DOWNGRADE` evidence (`STANDARD_24M`) so the offence is forensically visible. The downgrade does not fail the action; it just reroutes it onto the fairness-bounded queue.

## §12 Adversarial robustness

This section enumerates the closed threat model and the fail-closed responses. Every defence below is constitutional; bundle authors cannot disable them.

### §12.1 Idempotency replay

The same `idempotency_key` reused with a different `request_hash` (S0.1 §3.3) is detected at `ValidateAction`. The runtime emits `IDEMPOTENCY_KEY_REPLAY_DETECTED` evidence (`EXTENDED_60M`) and rejects the new envelope with `RuntimeErrorCode = IDEMPOTENCY_REPLAY`. The earlier envelope is unaffected.

### §12.2 Adapter manifest forgery

A manifest whose signature does not verify against the publisher key trust chain is rejected at registration (`MANIFEST_SIGNATURE_INVALID`) and emits `ADAPTER_REGISTRATION_REJECTED` at FOREVER retention. The adapter does not appear in `ListAdapters`. A subsequent `ExecuteAction` referencing an action kind only that adapter would have served fails with `UNKNOWN_ACTION_KIND`.

### §12.3 Envelope tampering

An envelope whose schema validation fails — including an envelope with hash drift between submission and `ExecuteAction` — is rejected with `ENVELOPE_VALIDATION_FAILED` evidence and `RuntimeErrorCode = INVALID_ENVELOPE`. The drift case is the most common in practice: a caller resubmitted a mutated envelope hoping the runtime would honour the original approval. Step 1 of the eight-step pre-dispatch (§6.1) closes that path.

### §12.4 Approval-binding hash mismatch

If `bound_action_canonical_hash` on the binding differs from `hex_lower(BLAKE3(JCS(envelope.request)))[:32]` at `ExecuteAction`, the action is rejected with `BINDING_VOIDED_ACTION_REVISED` evidence at FOREVER retention. The binding is voided per S5.3 §13.2 / S5.4 §5; a new request must be authored. There is no implicit recovery from a single-byte mutation; this is the constitutional anti-replay rule and it is intentional.

### §12.5 Adapter exceeding declared kinds

An adapter that emits a result for an action kind it did not declare in its manifest is detected at response validation. The runtime rejects the response, transitions the action to `FAILED` with `ADAPTER_REFUSED`, and de-registers the adapter with `ADAPTER_DEREGISTERED` at `EXTENDED_60M` retention. The adapter is also marked `ADAPTER_DEGRADED`. Re-registration requires a fresh manifest signed by an authorised publisher.

### §12.6 Evidence chain failure during execute

If the evidence log returns `EvidenceWriteFailed` mid-execute (S0.1 §7.3 canonical code), the runtime transitions to `VERIFYING_BLOCKED_BY_EVIDENCE_TAMPER` (a state reachable only as an internal degraded marker; lifecycle exposed via `GetActionStatus.degraded_reason`), pauses the action, and raises a high-priority operator alert. The action does not progress to `SUCCEEDED` while the evidence chain is degraded — INV-014 forbids unproven completion.

### §12.7 AI-origin action attempting INTERACTIVE queue

Per §11.4, the action is silently downgraded to `AGENT_PROPOSAL` and the downgrade is evidence-logged. The action is not failed — silent downgrades preserve operational fluency for AI agents while keeping fairness invariants intact.

### §12.8 Adapter latency budget exceeded

An adapter that exceeds its declared `timeout_seconds` is interrupted by the runtime. The action transitions to `FAILED` with `ADAPTER_TIMEOUT`. The adapter's health is marked `ADAPTER_DEGRADED`; subsequent dispatches against it queue behind the degradation backpressure budget. Three timeouts within a 5-minute window mark the adapter `ADAPTER_UNHEALTHY`; its dispatches are paused entirely until an operator clears the health flag.

### §12.9 Adapter panic

An adapter process exit (`ADAPTER_PANIC`) is treated identically to `ADAPTER_TIMEOUT` for health tracking, with the additional observation that the adapter's dispatch_kind is forced to `SUBPROCESS_FORK` for the next 24 hours regardless of manifest declaration (defence in depth: a panicking in-process adapter is too risky to keep in-process).

### §12.10 Subject-cert binding mismatch

The S0.1 §10.6 subject-cert binding is enforced at the public ingress layer; this sub-spec inherits the rule. `request.subject` must match the gRPC mTLS client cert, or the peer must be authorised to act-as the subject (act-as policy in L4). Mismatch → `PERMISSION_DENIED`; envelope not accepted; no L3 lifecycle is created.

### §12.11 Manifest-bypass attempt

A direct dispatch attempt without a valid manifest entry (e.g. a caller invoking `ExecuteAction` on an action kind whose adapter is `RETIRED`) is rejected with `UNKNOWN_ACTION_KIND` or `ADAPTER_NOT_DISPATCHABLE`. The runtime never falls back to a "default adapter" or a "best-guess adapter"; unsupported actions fail closed (`00_overview.md` invariant).

## §13 Evidence record types

This sub-spec emits the closed list of twenty evidence record types below. Each is **queued for addition** to the closed S3.1 `RecordType` enum (S3.1 §24.1 currently terminates at 87 entries; the twenty additions advance the count by 20 once consolidated). Until S3.1 is amended, the runtime emits these record types as `RECORD_TYPE_UNSPECIFIED` with a typed payload extension; the consolidation pass will rewire them to the canonical enum values.

| Record type                          | Default retention | Emitted on                                                                                                          |
| ------------------------------------ | ----------------- | ------------------------------------------------------------------------------------------------------------------- |
| `ACTION_RECEIVED`                    | `STANDARD_24M`    | `ValidateAction` accepted the envelope; `CREATED` lifecycle state recorded.                                         |
| `ACTION_VALIDATED`                   | `STANDARD_24M`    | `ValidateAction` finished schema, target, sandbox, and verification grammar checks.                                 |
| `ACTION_POLICY_DECISION`             | `STANDARD_24M`    | `EvaluatePolicyForAction` recorded the policy decision against the action. Mirrored to L9.1 `POLICY_DECISION`.      |
| `ACTION_DISPATCHED`                  | `STANDARD_24M`    | The eight-step pre-dispatch (§6.1) succeeded and the adapter was invoked.                                           |
| `EXECUTION_SUCCEEDED`                | `STANDARD_24M`    | Adapter returned `ADAPTER_OK` and verification returned `VERIFICATION_PASSED` for all intents.                      |
| `EXECUTION_FAILED`                   | `EXTENDED_60M`    | Lifecycle transitioned to `FAILED`; payload carries `ExecutionFailureReason` and `current_canonical_hash`.          |
| `EXECUTION_VERIFICATION_FAILED`      | `EXTENDED_60M`    | Verification returned a non-`PASSED` status; payload carries the failing intent and observed state (redacted).      |
| `ROLLBACK_ATTEMPTED`                 | `STANDARD_24M`    | `RollbackAction` invoked the adapter's rollback; payload carries `RollbackStrategy` and pre-state hash.             |
| `ROLLBACK_SUCCEEDED`                 | `STANDARD_24M`    | Rollback returned `RollbackOutcome.SUCCEEDED`; lifecycle transitioned to `ROLLED_BACK`.                             |
| `ROLLBACK_FAILED_REQUIRES_OPERATOR`  | `FOREVER`         | Rollback returned `RollbackOutcome.FAILED`; lifecycle transitioned to `ROLLBACK_FAILED`; affected_resources listed. |
| `ADAPTER_REGISTERED`                 | `STANDARD_24M`    | Adapter manifest accepted at registration; lifecycle starts at `REGISTERED` stability.                              |
| `ADAPTER_REGISTRATION_REJECTED`      | `FOREVER`         | Manifest signature failed, publisher unrecognised, or expired; constitutional forensic event.                       |
| `ADAPTER_DEGRADED`                   | `STANDARD_24M`    | Adapter health transitioned to `ADAPTER_DEGRADED` (rate of timeout / panic / kind-overrun).                         |
| `ADAPTER_DEREGISTERED`               | `EXTENDED_60M`    | Adapter de-registered (manifest expired, kind-overrun voided, operator action).                                     |
| `IDEMPOTENCY_KEY_REPLAY_DETECTED`    | `EXTENDED_60M`    | Same `idempotency_key` with different `request_hash` observed at `ValidateAction`.                                  |
| `BINDING_VOIDED_ACTION_REVISED`      | `FOREVER`         | Eight-step step 1 detected canonical-hash drift; binding voided per S5.3 §13 / S5.4 §5.                             |
| `AI_INTERACTIVE_QUEUE_DOWNGRADE`     | `STANDARD_24M`    | AI subject silently downgraded from `INTERACTIVE` to `AGENT_PROPOSAL`.                                              |
| `DRY_RUN_SIMULATION_RECORDED`        | `STANDARD_24M`    | `SIMULATE` action terminated with a simulation transcript; segregated from production evidence stream.              |
| `EXPERIMENTAL_ADAPTER_LIVE_DISPATCH` | `EXTENDED_60M`    | An action against an `EXPERIMENTAL` adapter dispatched live (not `DRY_RUN`); operator clearance required.           |
| `ADAPTER_DEPRECATED_DISPATCH`        | `STANDARD_24M`    | An action against a `DEPRECATED` adapter dispatched; operational signal that the adapter should be retired.         |

Each record's payload includes the relevant ids (`action_request_id`, `adapter_id`, `policy_decision_id`, `approval_request_id`, `binding_id`, `override_id`), the lifecycle state at emission, and the redacted-by-default observed state. Secret-shaped redaction follows S3.1 §14 default profile. INV-015 (evidence never contains secrets) binds every payload.

The runtime is the **only** authorised emitter for these twenty record types. Append attempts from any other subject are hard-denied at the evidence log surface and themselves emit `TAMPER_DETECTED` per S3.1 §11.5.

## §14 Performance contract

### §14.1 Per-RPC budgets

| RPC                                    | p95                 | Hard timeout | Notes                                                                                   |
| -------------------------------------- | ------------------- | ------------ | --------------------------------------------------------------------------------------- |
| `ValidateAction`                       | < 5 ms              | 50 ms        | Schema, target, sandbox, verification grammar checks. No external I/O.                  |
| `EvaluatePolicyForAction`              | < 25 ms             | 200 ms       | Includes `PolicyKernel.EvaluatePolicy` p95 (S2.3 §18.1).                                |
| `RequestApprovalForAction`             | depends on operator | 24 h max     | Bounded by `ApprovalTtlClass` ceiling (S5.3 §3.6); not budget-shaped (operator-driven). |
| `ExecuteAction` (synchronous overhead) | < 25 ms             | 200 ms       | Excludes adapter execution. Covers the eight-step pre-dispatch.                         |
| `VerifyAction` (overhead)              | < 50 ms             | 500 ms       | Excludes probe execution per S2.4 §9.1.                                                 |
| `RollbackAction` (overhead)            | < 25 ms             | 200 ms       | Excludes adapter rollback handler.                                                      |
| `GetActionStatus`                      | < 10 ms             | 100 ms       | Index lookup.                                                                           |
| `ListAdapters`                         | < 10 ms             | 100 ms       | In-memory directory.                                                                    |
| `GetAdapterCapabilities`               | < 10 ms             | 100 ms       | In-memory directory.                                                                    |
| `GetCapabilityRuntimeInfo`             | < 5 ms              | 100 ms       | Metric snapshot.                                                                        |

### §14.2 Adapter response timeout

Adapter-side timeout is **adapter-declared**, bounded by the runtime:

| Bound                                      | Default | Maximum |
| ------------------------------------------ | ------- | ------- |
| `AdapterActionDeclaration.timeout_seconds` | 30 s    | 300 s   |

A manifest declaring `timeout_seconds = 0` or `> 300 s` fails registration with `MANIFEST_SIGNATURE_INVALID` and `manifest_invalid_field = timeout_seconds`. Long-running operations belong in the `BACKGROUND` queue with a separate completion-watch mechanism (out of scope for Rev.2).

### §14.3 Queue health threshold

| Queue depth | State                                  |
| ----------- | -------------------------------------- |
| 0 – 800     | Healthy                                |
| 801 – 1 000 | Approaching threshold (operator alert) |
| > 1 000     | Backpressure mode (§11.3)              |

### §14.4 Telemetry contract

Bounded label cardinality (per S3.1 §20 conventions; subject ids never appear as labels):

| Metric                                     | Type      | Labels (closed)                                            |
| ------------------------------------------ | --------- | ---------------------------------------------------------- |
| `runtime_actions_total`                    | counter   | `terminal_state` (4 values), `dispatch_kind` (4 values)    |
| `runtime_action_lifecycle_latency_seconds` | histogram | `from_state`, `to_state` (closed pairs from §4.2)          |
| `runtime_pre_dispatch_step_failures_total` | counter   | `step` (1..8), `failure_reason` (12 values from §3.6)      |
| `runtime_adapter_health`                   | gauge     | `adapter_id` (bounded ≤ 200 adapters), `health` (3 values) |
| `runtime_queue_depth`                      | gauge     | `queue_class` (4 values)                                   |
| `runtime_queue_backpressure_active`        | gauge     | none                                                       |
| `runtime_evidence_chain_degraded`          | gauge     | none                                                       |
| `runtime_idempotency_replay_total`         | counter   | none                                                       |
| `runtime_binding_voided_total`             | counter   | `reason` (closed: `ACTION_REVISED`, `EXPIRED`, `REVOKED`)  |
| `runtime_ai_interactive_downgrade_total`   | counter   | none                                                       |
| `runtime_adapter_dispatch_total`           | counter   | `adapter_id`, `dispatch_kind`                              |
| `runtime_adapter_dispatch_latency_seconds` | histogram | `adapter_id`, `dispatch_kind`                              |
| `runtime_rollback_outcome_total`           | counter   | `outcome` (4 values from §3.7)                             |
| `runtime_dry_run_simulations_total`        | counter   | `dispatch_kind` (always `DRY_RUN`)                         |
| `runtime_experimental_dispatch_total`      | counter   | `adapter_id`                                               |

Cardinality budget: ≤ 2 000 active label tuples across the full set (dominated by `adapter_id` × `dispatch_kind` × `from_state`/`to_state` combinations). Adapter ids are bounded by registration: an adapter that is `RETIRED` and has not dispatched in 30 days is dropped from the metric label set.

## §15 Cross-references

| Spec                                                                                                   | Direction  | Relationship                                                                                                                                                                                                          |
| ------------------------------------------------------------------------------------------------------ | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [S0.1 Action Envelope + Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)                | consumer   | Wire shape, canonical hash convention, public phase enum, public ingress (`SubmitAction` etc.).                                                                                                                       |
| [S2.3 Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)                                  | consumer   | `EvaluatePolicy` called from `EvaluatePolicyForAction`; `Constraints` consumed; `bundle_version` re-checked at step 2 of pre-dispatch.                                                                                |
| [S5.3 Approval Mechanics](../L4_Policy_Identity_Vault/04_approval_mechanics.md)                        | consumer   | `ApprovalRequest` constructed and submitted; `ApprovalBinding` consumed at step 7 of pre-dispatch; FSM observed by `RequestApprovalForAction`.                                                                        |
| [S5.4 Emergency Override](../L4_Policy_Identity_Vault/05_emergency_override.md)                        | consumer   | `OverrideRequest` / `OverrideBinding` consumed; `OS_ACTIVE → OS_CONSUMED` transition; `NonOverridableClass` enforced; `STRONG_SOLO` recovery-only path inherited.                                                     |
| [S5.2 Vault Broker](../L4_Policy_Identity_Vault/02_vault_broker.md)                                    | consumer   | Capability re-checked at step 4 of pre-dispatch; `vault_capability_handle` passed to adapter without raw material; INV-018 honoured.                                                                                  |
| [S5.1 Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)                                | consumer   | Subject canonical form; `is_ai` flag drives dispatch decision and queue downgrade; mTLS subject binding at the public ingress.                                                                                        |
| [S2.4 Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)           | consumer   | `VerificationEngine.RunVerification` called from `VerifyAction`; result statuses projected into the L3 lifecycle.                                                                                                     |
| [S3.1 Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)                           | producer   | All twenty `ACTION_*`, `EXECUTION_*`, `ROLLBACK_*`, `ADAPTER_*`, `IDEMPOTENCY_*`, `BINDING_*`, `AI_*`, `DRY_RUN_*`, `EXPERIMENTAL_*` records appended through `Append`; retention floor declared per type.            |
| [S3.2 Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)                | consumer   | `ComposeProfile` re-evaluated at step 5 of pre-dispatch; `applied_sandbox_profile_id` recorded on the envelope; floor enforcement honoured.                                                                           |
| [L0.4 Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md)                    | enforcer   | INV-002 (AI proposes never executes), INV-005 (evidence append-only), INV-007 (layers depend downward only), INV-009 (approvals bind and expire), INV-013 (AI cannot system-admin), INV-014 (no proof no completion). |
| [L3 Overview](./00_overview.md)                                                                        | sibling    | Headline status, layer invariants (no free-form shell, fail closed, runtime correctness without LLM).                                                                                                                 |
| [L3 Unit Manifest](./01_unit_manifest.md)                                                              | sibling    | Service unit schema (out of scope here).                                                                                                                                                                              |
| [L3 State Transitions](./02_state_transitions.md)                                                      | sibling    | Desired-state graph evaluation (out of scope here).                                                                                                                                                                   |
| [L3 Adapter Model](./04_adapter_model.md)                                                              | sibling    | Per-adapter implementation patterns (out of scope here).                                                                                                                                                              |
| [Rev.1 §10 — AIOS-SGR](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)                             | supersedes | This sub-spec is the rev.2 contract-grade refinement of Rev.1's runtime narrative.                                                                                                                                    |
| [Rev.1 §13 — Typed Actions and Capability Runtime](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md) | supersedes | This sub-spec re-instantiates Rev.1's nine-RPC orchestration model on top of the S0.1 envelope.                                                                                                                       |

## §16 Worked examples

These examples walk the FSM end-to-end under three concrete scenarios. Times are illustrative; identifiers are fabricated.

### §16.1 Operator approves `aios.fs.write` to a group object — happy path

Setup. Group `family`. Human user `family:alice` is logged in at the local KDE console with `session_class = STRONG`. The action target is an object under `groups/family/shared/notes/2026-05-09.md` with `privacy_class = INTERNAL`. The action's `risk.privileged = true` because the policy bundle requires elevation for writes to `INTERNAL` objects.

**Step 1 — `ValidateAction`.** Alice's tooling submits the envelope. The runtime validates schema, target, sandbox profile id (empty → adapter default), and verification grammar (`object.exists`, `object.contents.contains "2026-05-09"`). Lifecycle: (init) → `CREATED`. Evidence: `ACTION_RECEIVED` and `ACTION_VALIDATED`.

**Step 2 — `EvaluatePolicyForAction`.** The runtime calls `PolicyKernel.EvaluatePolicy`. The kernel returns `REQUIRE_APPROVAL` with `ApprovalRequirement{ approver_classes = ["human"], strength = STRONG, ttl_seconds = 300 }`. Reason code `ScopedAllow` upgraded to `RequireApproval` per the bundle rule `family.write.requires.approval`. Lifecycle: `CREATED → POLICY_PENDING → APPROVAL_PENDING`. Evidence: `ACTION_POLICY_DECISION`.

**Step 3 — `RequestApprovalForAction`.** The runtime hands the requirement to L4.3. L4.3 selects `KDE_NATIVE_PROMPT` and delivers the prompt to alice's session. Alice approves. L4.3 issues an `ApprovalBinding` with `bound_action_canonical_hash = H_grant`. Lifecycle: `APPROVAL_PENDING → APPROVED`. Evidence: `APPROVAL_REQUESTED`, `APPROVAL_DELIVERED`, `APPROVAL_GRANTED` (all from L4.3).

**Step 4 — Queue.** The runtime enrols the action in `INTERACTIVE` (alice is human). Lifecycle: `APPROVED → QUEUED`.

**Step 5 — `ExecuteAction`.** The eight-step pre-dispatch runs:

1. Canonical hash matches `H_grant`. ✓
2. Bundle version unchanged. ✓
3. Binding state = `GRANTED`. ✓
4. No vault capability required for this action. ✓
5. Sandbox profile re-composed; `applied_sandbox_profile_id = sb:fs_write_internal`. ✓
6. Lifecycle in `QUEUED`. ✓
7. Binding marked `CONSUMED`. Evidence: `APPROVAL_CONSUMED` (from L4.3).
8. Adapter `adapter:aios:fs:1.0.0` dispatched under `SUBPROCESS_FORK` (alice is human, `risk.privileged = true` but human privilege is not AI; `SUBPROCESS_FORK` per §3.2 fallback).

Lifecycle: `QUEUED → EXECUTING`. Evidence: `ACTION_DISPATCHED`.

**Step 6 — Adapter execute.** The fs adapter writes the version, promotes the pointer atomically (S1.3 §6 CAS), and returns `ADAPTER_OK` with `changed = true` and a `rollback_handle` pointing at the prior pointer state.

**Step 7 — `VerifyAction`.** Lifecycle: `EXECUTING → VERIFYING`. The verification engine runs both intents:

- `object.exists` against the new version → `VERIFICATION_PASSED`.
- `object.contents.contains "2026-05-09"` → `VERIFICATION_PASSED`.

Lifecycle: `VERIFYING → SUCCEEDED`. Evidence: `EXECUTION_SUCCEEDED`.

**Step 8 — Final.** The full evidence chain — `ACTION_RECEIVED → ACTION_VALIDATED → ACTION_POLICY_DECISION → APPROVAL_REQUESTED → APPROVAL_DELIVERED → APPROVAL_GRANTED → APPROVAL_CONSUMED → ACTION_DISPATCHED → EXECUTION_SUCCEEDED` — is reconstructible from L9.1 by `correlation_id`. INV-014 satisfied: every transition is evidence-linked, the verification proof is present, and the action is `SUCCEEDED`.

### §16.2 AI proposes `policy.bundle.update` (hard-denied) → operator override → succeeds

Setup. AI agent `homelab:assistant` proposes a `policy.bundle.update` action to relax a scoped DENY rule. The hard-deny `hd.disable_policy_kernel` is registered for any action that would disable the kernel itself, but `policy.bundle.update` is a scoped DENY (the kernel can be updated; the rule prohibits AI subjects from doing it directly). The hard-deny rule id `policy.update.requires_human` is **not** in `NonOverridableClass` (S5.4 §10). Operators alice and bob are present.

**Step 1 — `ValidateAction`.** Submitted by the agent. Lifecycle: → `CREATED`. Evidence: `ACTION_RECEIVED`, `ACTION_VALIDATED`.

**Step 2 — `EvaluatePolicyForAction`.** Policy returns `DENY` with `reason_code = AISelfApprovalPrevented` (per S2.3 §17). Lifecycle: `CREATED → POLICY_PENDING → POLICY_DENIED`. Evidence: `ACTION_POLICY_DECISION` (decision = DENY, FOREVER).

**Step 3 — Operator authors override.** Alice authors an `OverrideRequest` with `target_action_canonical_hash` matching the action, `strength = DUAL_HUMAN`, `scope = ONE_ACTION`, `ttl_class = TTL_OVERRIDE_INSTANT`. The runtime transitions the lifecycle: `POLICY_DENIED → OVERRIDE_PENDING` (T21).

**Step 4 — Override quorum.** Alice signs on KDE. Bob signs on Web loopback. L4.5 verifies channel separation (`KDE_NATIVE_PROMPT ≠ WEB_LOOPBACK_PROMPT`), subject distinctness (alice ≠ bob), and both human subjects. `OverrideBinding` issued. Lifecycle: `OVERRIDE_PENDING → APPROVED`. Evidence: `OVERRIDE_REQUESTED`, `OVERRIDE_QUORUM_RECEIVED`, `OVERRIDE_GRANTED` (all FOREVER, from L4.5).

**Step 5 — Queue.** Lifecycle: `APPROVED → QUEUED`. Queue class is `AGENT_PROPOSAL` (the action's emitter is the AI agent, even though the override authority is human).

**Step 6 — `ExecuteAction`.** Eight-step pre-dispatch with `OverrideBinding` substituted for `ApprovalBinding`:

1. Canonical hash matches. ✓
2. Bundle version unchanged. ✓
3. Override binding state = `OS_ACTIVE`. ✓
4. No vault capability required. ✓
5. Sandbox composed; `applied_sandbox_profile_id = sb:policy_admin_isolated`. ✓
6. Lifecycle in `QUEUED`. ✓
7. Override binding marked `OS_CONSUMED`. Evidence: `OVERRIDE_CONSUMED` (FOREVER, from L4.5).
8. Adapter `adapter:aios:policy:1.0.0` dispatched under `ISOLATED_SANDBOX` (subject `is_ai = true` forces it).

Lifecycle: `QUEUED → EXECUTING`. Evidence: `ACTION_DISPATCHED`.

**Step 7 — Adapter execute.** The policy adapter applies the bundle update, returns `ADAPTER_OK`.

**Step 8 — `VerifyAction`.** Verification runs; bundle version on disk now matches the requested update; `VERIFICATION_PASSED`. Lifecycle: `EXECUTING → VERIFYING → SUCCEEDED`. Evidence: `EXECUTION_SUCCEEDED`.

INV-002, INV-009, INV-014 all satisfied. INV-007 enforced: hard-deny was loud (FOREVER override evidence), costly (DUAL_HUMAN, two channels, cooldown), and never silent.

### §16.3 Adapter execute succeeds; verification fails → checkpoint-based rollback succeeds

Setup. Operator submits `service.restart nginx` after editing a vhost. Adapter `adapter:systemd:local:1.2.0` declares `rollback_strategy = CHECKPOINT_BASED` for `service.restart`. Verification intent: `service.active nginx` and `http.ok http://localhost/`.

**Step 1–5.** Standard happy path through `CREATED → APPROVED → QUEUED → EXECUTING`. Eight-step pre-dispatch passes.

**Step 6 — Adapter execute.** systemd restarts nginx. The unit file references a vhost that fails to parse. nginx exits during start. Adapter returns `ADAPTER_OK` (the systemd unit transitioned through `start-pre`/`start` and the adapter observed the unit reach `active (running)` momentarily before failure — this is a known systemd race). The runtime moves to `VERIFYING`. Evidence: `ACTION_DISPATCHED`.

**Step 7 — Verification fails.** `service.active nginx` returns `VERIFICATION_FAILED` (state is `failed`). `http.ok http://localhost/` returns `VERIFICATION_FAILED` (connection refused). Lifecycle: `VERIFYING → FAILED`. Evidence: `EXECUTION_VERIFICATION_FAILED` (`EXTENDED_60M`).

**Step 8 — Rollback.** Manifest declares `rollback_strategy = CHECKPOINT_BASED`. The runtime calls `Rollback(action_request_id, rollback_handle)`. The systemd adapter restores the previous unit-file checkpoint and restarts nginx. The checkpoint restore succeeds; nginx becomes `active (running)`. Adapter returns `RollbackOutcome.SUCCEEDED`. Lifecycle: `FAILED → ROLLED_BACK`. Evidence: `ROLLBACK_ATTEMPTED` then `ROLLBACK_SUCCEEDED`.

The full evidence chain reconstructs the entire history: the action was attempted, it failed verification, the system rolled back to a known-good state. The operator's interactive session shows the action as `ROLLED_BACK` with the original error in `Error.cause` and the rollback summary in `Result.summary`. INV-014 honoured: even rollback is evidence-proven.

If instead the adapter's `Rollback()` had returned `RollbackOutcome.FAILED`, the lifecycle would have transitioned to `ROLLBACK_FAILED` (terminal) with `ROLLBACK_FAILED_REQUIRES_OPERATOR` evidence at FOREVER retention, the systemd adapter would have been marked `ADAPTER_DEGRADED`, and an operator alert would have been raised through L9.4.

## §17 Open deferrals

These items are intentionally out of scope for S10.1 and tracked elsewhere or queued for future revisions:

- **Sub-action saga composition** — fan-out/fan-in across multiple actions tied to one user intent. Deferred to a future cross-cutting sub-spec; the S0.1 single-parent causality model is sufficient for Rev.2.
- **Multi-host capability federation** — alice on host A approves an action whose adapter dispatches on host B. Out of scope; depends on multi-host identity federation (S5.1 §19) and policy bundle distribution semantics (S2.3 future revision).
- **Streaming adapter responses** — for long-running actions where partial output is operationally valuable. Currently bounded by the 300 s manifest maximum; long-running operations live in `BACKGROUND` with separate completion-watch primitives. A streaming `ExecuteAction` variant is queued for a future revision.
- **Action TTL / queue expiry** — bounded queue residency for actions whose business deadline has passed. Currently the eight-step pre-dispatch catches stale bindings (via `BINDING_EXPIRED`); a richer TTL model on the queue itself is queued.
- **Resource-budget hints** — caller-side hints about expected execution duration / memory / network use, used by the queue to make admission-control decisions. Out of scope for Rev.2.
- **Adapter publisher endorsement model** — full PKI for adapter manifest signing. Currently inherits the policy bundle trust chain (S2.3 §12.3); a dedicated adapter trust framework is queued.
- **Rollback chains** — rolling back a chain of causally linked actions atomically. Currently each action is rolled back independently; chain rollback is queued.
- **`ROLLBACK_FAILED` recovery runbook** — operator-facing procedures for diagnosing and resolving terminal rollback failures. Documented in operator runbooks (out of spec scope).
- **Multi-instance coordination** — when more than one runtime instance must agree on action ordering. Currently single-instance per host; multi-instance is queued.
- **Adversarial robustness fixtures** — golden fixtures that audit eight-step pre-dispatch under concurrent revoke/consume races, partition-induced binding drift, and clock-skew TTL edge cases. Queued for the S10.1 acceptance harness.

## §18 Acceptance criteria

- Default fail-closed works: an action whose validation fails never reaches policy.
- The fourteen-state lifecycle FSM admits only the listed transitions; forbidden transitions emit `LIFECYCLE_ILLEGAL_TRANSITION` evidence.
- The eight-step pre-dispatch is run in order on every `ExecuteAction`; skipping any step is a constitutional violation.
- Binding hash mismatch (S5.3 §13.2 / S5.4 §5) at step 1 voids the binding and emits FOREVER evidence.
- `ROLLBACK_FAILED` is terminal; it raises an operator alert and is not auto-retried.
- `NonOverridableClass` hard-denies cannot be reached through the override path; T21 only fires for non-`NonOverridableClass` rules.
- AI subjects cannot author trust-bearing approval prompts (inherited from L7.2 + S5.3); the runtime does not provide a backdoor.
- `EXPERIMENTAL` adapters dispatch live only with explicit operator clearance; `EXPERIMENTAL_ADAPTER_LIVE_DISPATCH` evidence is FOREVER.
- The free-form shell adapter input mode is constitutionally absent; manifest registration of an unbound template fails.
- The twenty evidence record types are emitted at the correct retention class.
- `AGENT_PROPOSAL` queue cap (50 % default) is enforced; AI subjects on `INTERACTIVE` are silently downgraded with evidence.
- All performance budgets in §14 are honoured under reference workload.
- The three worked examples in §16 reconstruct end-to-end from L9.1 alone.
- Idempotency replay (same key, different hash) is rejected with FOREVER evidence.
- Adapter manifest forgery is rejected at registration with FOREVER evidence.
- Subject-cert binding is enforced at the public ingress; mismatch never creates an L3 lifecycle.

## §19 Status & evidence grade

Status: REAL
Evidence: E1 (file exists; structural contract complete; signed off in spec rev.2 master index)

The next evidence step (E2) requires a typecheck-clean proto IDL extracted from this sub-spec into the `aios.runtime.v1alpha1` schema package, including the closed `ActionLifecycleState`, `ActionDispatchKind`, `AdapterIOMode`, `AdapterStability`, `QueueClass`, `ExecutionFailureReason`, `RollbackOutcome`, `RuntimeErrorCode`, and `RollbackStrategy` enums and the `AdapterManifest` record. The next step (E3) requires unit and integration tests against the FSM and the eight-step pre-dispatch sequence under all closed failure-reason values. The next step (E4) requires end-to-end execution of the three worked examples in §16 against a working Capability Runtime instance with real adapters, with all evidence reconstructible from L9.1. The full E5 (live operational) status is reached only after the runtime is deployed and producing evidence in non-simulation mode against multiple production adapters.

## Wave 8 cross-spec touch-up (Tier 1 + Tier 2 typed action catalog additions)

Applied 2026-05-11. Sources: [S12.1 App Runtime Model](../L6_Apps_Packages_Compatibility/01_app_runtime_model.md), [S9.3 Dedicated Kernel Pipeline](../L1_Kernel_Bootstrap_Recovery/03_dedicated_kernel_pipeline.md), [S12.4 Compatibility Knowledge](../L6_Apps_Packages_Compatibility/05_compatibility_knowledge.md), [S8.3 Hardware Graph](../L8_Network_Hardware_Devices/01_hardware_graph.md), [S8.4 DNS / VPN / mDNS Management](../L8_Network_Hardware_Devices/03_dns_vpn_management.md), [S8.5 Firmware Trust](../L8_Network_Hardware_Devices/04_firmware_trust.md), [S11.3 External Integrations](../L10_Distribution_Ecosystem_Marketplace/03_external_integrations.md), [S13.2 Model Router](../L5_Cognitive_Core/05_model_router.md), [S9.2 First-Boot Flow](../L1_Kernel_Bootstrap_Recovery/02_first_boot_flow.md). This section consolidates every typed action explicitly queued for S10.1 by Tier 1 and Tier 2 source contracts (Wave 7 §26.6 and the Tier 2 cross-spec impact notes). It is **additive**: no existing §3 enum or §4 lifecycle is redefined. Every action below binds to the existing closed `ActionDispatchKind`, `AdapterIOMode`, and `AdapterStability` vocabulary. Each action's RecordType emissions have already been consolidated into the S3.1 Wave 8 RecordType enum; this contract does not redefine evidence shape.

### W8.1 New typed actions per source spec

Each subsection lists the actions queued by exactly one source. The columns are:

- **action_kind** — dotted name; bundle-load fails on unknown (per §3 closed-vocabulary discipline);
- **AdapterIOMode** — `TYPED_PARAMETERS_ONLY` or `TEMPLATE_PARAMETERS` (closed enum, §3.3);
- **Permission** — `HUMAN_USER` (S5.3 EXACT_ACTION binding required at dispatch), `AI_SUBJECT_OK` (system service or AI-origin subject permitted by the source spec under standing approval or the propose-only INV-002 contract), `RECOVERY_ONLY` (`request.environment` MUST be `RECOVERY` per S0.1; non-recovery dispatches are hard-denied with `RecoveryRequiredForSystemMutation`);
- **Source spec** — the contract that introduced and queued the action;
- **Purpose** — single-sentence operational meaning.

All actions in this Wave dispatch as `ISOLATED_SANDBOX` (per §3.2: any AI-origin action AND any system-service action whose target resolves under `/aios/system/...` is forced to isolated sandbox). All actions are `STABLE` once their adapter is registered; until then they inherit `EXPERIMENTAL` per §3.4.

#### W8.1.1 — S12.1 App Runtime Model (4 actions)

Queued by [S12.1 §8.2](../L6_Apps_Packages_Compatibility/01_app_runtime_model.md). Originally promised in Wave 7 §26.6 against this sub-spec; consolidated here.

| action_kind                  | AdapterIOMode           | Permission      | Source spec | Purpose                                                                                                                                                                                                                                                                       |
| ---------------------------- | ----------------------- | --------------- | ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `app.observe_in_sandbox`     | `TYPED_PARAMETERS_ONLY` | `AI_SUBJECT_OK` | S12.1 §6    | Phase A — run a foreign-ecosystem app inside the maximally-restricted observation sandbox to record `ObservedBehavior`; subject is `_system:service:app-observer`.                                                                                                            |
| `app.translate_manifest`     | `TYPED_PARAMETERS_ONLY` | `AI_SUBJECT_OK` | S12.1 §7    | Phase B — AI proposer derives `SandboxProfile + NetworkOutboundManifest + capability list` from the foreign artifact via a closed `ManifestTranslationStrategy`; subject is the AI agent (`is_ai = true`); INV-002 binds — execution is the proposal record, not the install. |
| `app.propose_manifest_delta` | `TYPED_PARAMETERS_ONLY` | `AI_SUBJECT_OK` | S12.1 §7.4  | Phase D — AI proposer emits a `ManifestDeltaOutcome` proposal recommending capability or sandbox adjustments based on Phase C audit signals; INV-002 binds.                                                                                                                   |
| `app.contribute_recipe`      | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | S12.1 §7.5  | Operator-initiated contribution of a successfully-installed app's recipe back to the Community Recipe Registry; AI cannot contribute on behalf of the operator (INV-002 envelope-FSM rejection on `policy_pending → executing`).                                              |

#### W8.1.2 — S9.3 Dedicated Kernel Pipeline (2 actions)

Queued by [S9.3 §8.1, §11.1](../L1_Kernel_Bootstrap_Recovery/03_dedicated_kernel_pipeline.md). Originally promised in Wave 7 §26.6 against this sub-spec; consolidated here.

| action_kind      | AdapterIOMode           | Permission      | Source spec | Purpose                                                                                                                                                                                                                                                                                                                                                                                    |
| ---------------- | ----------------------- | --------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `kernel.build`   | `TYPED_PARAMETERS_ONLY` | `AI_SUBJECT_OK` | S9.3 §8.1   | Run the `KernelBuilder.Build` pipeline (Build + Convergence + Six Gates) and produce a `KernelImage`. Subject is `_system:service:kernel-builder` (`is_ai = false`); approval is the standing pipeline-definition approval, not per-invocation; per-invocation operator approval is required only at separate `kernel.promote_to_a` (which is RECOVERY_ONLY and out of this Wave's scope). |
| `kernel.refresh` | `TYPED_PARAMETERS_ONLY` | `AI_SUBJECT_OK` | S9.3 §11.1  | Scheduled L3 SGR action that resolves the latest pinned upstream stable tag and dispatches `kernel.build` against it. Same subject and approval discipline as `kernel.build`; emits `KERNEL_REFRESH_SCHEDULED` at start and `KernelMaintenanceResult` at finish.                                                                                                                           |

#### W8.1.3 — S12.4 Compatibility Knowledge (3 actions)

Queued by [S12.4 §8](../L6_Apps_Packages_Compatibility/05_compatibility_knowledge.md). All three flow through S5.3 `EXACT_ACTION` approval; AI subjects can populate the envelope but cannot drive `policy_pending → executing` (INV-002 binding analogous to `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED`).

| action_kind                             | AdapterIOMode           | Permission   | Source spec | Purpose                                                                                                                                                                                                                                                                                                                           |
| --------------------------------------- | ----------------------- | ------------ | ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `compat.contribute_profile_observation` | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER` | S12.4 §8    | Operator records a `RecipeRating` observation for an app + `EcosystemRuntime` pair with explicit `ProfileVisibility` disclosure; visibility upgrades are presented as a separate operator-comprehensible disclosure in the approval prompt.                                                                                       |
| `compat.import_profile_from_upstream`   | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER` | S12.4 §8    | Operator-initiated one-shot import of a snapshot profile from a closed upstream registry (`PROTONDB`, `WINEHQ_APPDB`, `FLATHUB`, `SNAPCRAFT`) under a `_system:bridge:<source>` system bridge subject; approval prompt names the source explicitly ("import N ProtonDB ratings as advisory metadata; no installs are performed"). |
| `compat.review_outlier_contribution`    | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER` | S12.4 §8    | Operator with `compat.review` capability (or AIOS-root governance) adjudicates a contribution flagged by the outlier detector (§5.4 of S12.4); outcome adjusts contributor weight and registry inclusion per §5.1.                                                                                                                |

#### W8.1.4 — S8.3 Hardware Graph (6 actions, post-W9 split)

Queued by [S8.3 §8.2 + §11](../L8_Network_Hardware_Devices/01_hardware_graph.md). Every mutating HDM RPC is a typed action under this contract. INV-013 is enforced at envelope validation: AI subjects are hard-denied with `AI_FORBIDDEN_OPERATION` and `AI_REMOVABLE_DEVICE_BLOCKED` FOREVER evidence before dispatch.

**W9 amendment** (Wave 9 constitutional surgery, Cluster 8 finding SIM-C-001 / CONS-S10-003): the original `hardware.accept_drift` (single HUMAN_USER entry) has been split into two scope-distinct actions to formalise the audit candidate W8.4.4 finding B6 (deferred from Wave 8). Drift on accessory hardware (laptop GPU swap, Wi-Fi card change, removable peripheral identity drift) remains HUMAN_USER consent in normal mode; drift on the constitutional substrate (CPU, TPM, BIOS/UEFI, firmware-bound discrete GPU) requires recovery-mode and the S5.4 hardware-key co-signer discipline. The S2.3 condition field `target.is_constitutional_substrate: bool` (derived from `device_class IN {CPU, TPM_2_0, BIOS_UEFI, GPU_DISCRETE_FIRMWARE_BOUND}`) is added by W9-A in S2.3 §26 alongside the new hard-deny rule `ConstitutionalSubstrateRequiresRecovery`.

| action_kind                                 | AdapterIOMode           | Permission      | Source spec    | Purpose                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ------------------------------------------- | ----------------------- | --------------- | -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `hardware.approve_removable_device`         | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | S8.3 §8.2      | Operator approves a removable device under a closed `RemovableDevicePolicy` class with bounded TTL; emits `REMOVABLE_DEVICE_APPROVED` evidence and writes the typed grant to `/aios/system/hardware/grants/<grant_id>`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `hardware.revoke_removable_device_approval` | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | S8.3 §8.4      | Operator explicitly revokes a previously-issued grant; transitions the device to `QUARANTINED` with reason `OPERATOR_INITIATED` and unbinds its driver.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `hardware.quarantine_device`                | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | S8.3 §6        | Operator forces a device into the `QUARANTINED` state for forensic review; emits `DEVICE_QUARANTINED` FOREVER evidence carrying the closed `DeviceQuarantineReason`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `hardware.rebind_driver`                    | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | S8.3 §5.3      | Operator forces re-evaluation of driver binding for a device; out-of-tree driver bind requires `RECOVERY_REQUIRED` per INV-012, but normal-mode in-tree rebind is permitted under operator approval.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `hardware.accept_drift_accessory`           | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | S8.3 §9.5 (W9) | Operator approves an unapproved cross-boot hardware-graph drift entry for non-substrate devices (ALL `DeviceClass` values EXCEPT `CPU`, `TPM_2_0`, `BIOS_UEFI`, `GPU_DISCRETE` when firmware-bound to the measured-boot chain). Normal-mode HUMAN_USER consent suffices; FOREVER evidence retained regardless of outcome via the new `HARDWARE_GRAPH_DRIFT_ACCEPTED` record (S8.3 §12, queued for S3.1 W10).                                                                                                                                                                                                                                                                                                                                                                       |
| `hardware.accept_drift_substrate`           | `TYPED_PARAMETERS_ONLY` | `RECOVERY_ONLY` | S8.3 §9.5 (W9) | Operator approves an unapproved cross-boot hardware-graph drift entry for constitutional substrate devices (`DeviceClass` ∈ {`CPU`, `TPM_2_0`, `BIOS_UEFI`, `GPU_DISCRETE_FIRMWARE_BOUND`}). Requires recovery-mode (`request.environment` = `RECOVERY` per S0.1) plus hardware-key co-signer per S5.4 emergency-override discipline; non-recovery dispatches are hard-denied via the S2.3 hard-deny `ConstitutionalSubstrateRequiresRecovery` rule (queued by W9-A in S2.3 §26) — emits `HARDWARE_SUBSTRATE_ACCEPT_OUTSIDE_RECOVERY_BLOCKED` (FOREVER, queued for S3.1 W10) on refused attempts. The SUCCESS path emits `HARDWARE_GRAPH_DRIFT_ACCEPTED` (FOREVER) with `is_substrate = true`. See also S8.5 §9.4 for the vault-rekey wiring on TPM/BIOS/CPU substrate acceptance. |

#### W8.1.5 — S8.4 DNS / VPN / mDNS Management (8 actions)

Queued by [S8.4 §11](../L8_Network_Hardware_Devices/03_dns_vpn_management.md). All resolver/VPN/mDNS mutations are typed actions under this contract. INV-002 binds (§3.4 of S8.4): AI-authored attempts are hard-denied at S2.3.

| action_kind                         | AdapterIOMode           | Permission      | Source spec | Purpose                                                                                                                                                                                                                 |
| ----------------------------------- | ----------------------- | --------------- | ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `network.resolver.set_backend`      | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | S8.4 §6.1   | Switch the active resolver backend among `SYSTEMD_RESOLVED`, `UNBOUND_LOCAL`, `DNSCRYPT_PROXY`; in-flight queries drained over a 5-second window.                                                                       |
| `network.resolver.set_allowlist`    | `TYPED_PARAMETERS_ONLY` | `RECOVERY_ONLY` | S8.4 §6.2   | Replace the signed resolver allowlist; recovery-mode-only per INV-012; outside recovery returns `RECOVERY_REQUIRED`.                                                                                                    |
| `network.vpn.establish_tunnel`      | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | S8.4 §6.3   | Operator establishes a WireGuard tunnel; non-WireGuard kinds are hard-denied via `OPERATOR_DEFINED_OTHER_BLACKLISTED`; activation requires `STRONG` approval strength.                                                  |
| `network.vpn.revoke_tunnel`         | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | S8.4 §5.4   | Operator revokes an active VPN tunnel; cascades to peer key cleanup and emits `VPN_TUNNEL_REVOKED` evidence.                                                                                                            |
| `network.vpn.rotate_peer_key`       | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | S8.4 §6.4   | Provider rotates its peer public key; signed by the provider's enrollment-time identity key (the signature is the authority — no extra `STRONG` approval required, but the rotation is recorded with FOREVER evidence). |
| `network.mdns.set_avahi_posture`    | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | S8.4 §5.6   | Operator sets the host-wide `MdnsAvahiPosture` (`DENY_DEFAULT` / `LAN_LOCAL_ALLOWED` / `RECOVERY_DENIED`).                                                                                                              |
| `network.mdns.grant_advertisement`  | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | S8.4 §6.5   | Per-service mDNS advertisement grant with TTL ≤ 30 days; requires `STRONG` approval strength; renewals re-emit the approval flow.                                                                                       |
| `network.mdns.revoke_advertisement` | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | S8.4 §6     | Operator revokes a previously-granted mDNS advertisement; immediate effect.                                                                                                                                             |

#### W8.1.6 — S8.5 Firmware Trust (1 action)

Queued by [S8.5 §4 + §8](../L8_Network_Hardware_Devices/04_firmware_trust.md). The seven-stage firmware update flow is a single typed action under this contract; the stages are internal to the action's adapter execution.

| action_kind               | AdapterIOMode           | Permission   | Source spec  | Purpose                                                                                                                                                                                                                                                                                                              |
| ------------------------- | ----------------------- | ------------ | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `firmware.update.request` | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER` | S8.5 §4 / §8 | Operator-authored firmware update flowing through the seven-stage pipeline (FETCH → VERIFY → APPROVE → STAGE → APPLY → VERIFY_POST → COMMIT_OR_ROLLBACK). AI subjects (`is_ai = true`) are hard-denied at envelope validation under S2.3 hard-deny `AISystemAdminBlocked`; rule id `hd.firmware_update.ai_authored`. |

#### W8.1.7 — S11.3 External Integrations (5 actions)

Queued by [S11.3 §11](../L10_Distribution_Ecosystem_Marketplace/03_external_integrations.md). Auto-bridge operations run under `_system:service:bridge-<source>` system bridge subjects (`is_ai = false`); the explicit OCI fetch path requires HUMAN_USER per the deferred §6.2 carve-out.

| action_kind                 | AdapterIOMode           | Permission      | Source spec | Purpose                                                                                                                                                                               |
| --------------------------- | ----------------------- | --------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bridge.fetch`              | `TYPED_PARAMETERS_ONLY` | `AI_SUBJECT_OK` | S11.3 §4    | System bridge fetches an upstream artifact (DEB, RPM, Flatpak, OCI metadata) from a pinned upstream mirror under the closed `BridgeSource` enum; subject cannot install directly.     |
| `bridge.repackage`          | `TYPED_PARAMETERS_ONLY` | `AI_SUBJECT_OK` | S11.3 §4    | System bridge repackages the upstream artifact into an AIOS package with attribution preserved; output flows to S11.1's standard install pipeline.                                    |
| `bridge.import_metadata`    | `TYPED_PARAMETERS_ONLY` | `AI_SUBJECT_OK` | S11.3 §6    | System bridge imports upstream catalogue metadata (e.g. GHCR container catalogue) for L7 marketplace surface display; importing metadata never authorises an install.                 |
| `bridge.import_recipe`      | `TYPED_PARAMETERS_ONLY` | `AI_SUBJECT_OK` | S11.3 §4    | System bridge imports an upstream recipe (Flathub manifest, AUR PKGBUILD) into the Community Recipe Registry, with attribution preserved as `RECIPE_IMPORTED`.                        |
| `bridge.oci.fetch_explicit` | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | S11.3 §6.2  | Operator-explicit OCI image fetch gated by S5.3 `EXACT_ACTION` approval; runs through a separate admission pipeline outside the auto-bridge (full UX queued for S11.2 consolidation). |

#### W8.1.8 — S13.2 Model Router (2 actions)

Queued by [S13.2 Appendix A](../L5_Cognitive_Core/05_model_router.md). The model router's hot path (`Invoke`) is **not** a typed action — it is a runtime dispatch under the standing routing precedence; only adapter lifecycle is action-mediated.

| action_kind                     | AdapterIOMode           | Permission   | Source spec | Purpose                                                                                                                                                                                                 |
| ------------------------------- | ----------------------- | ------------ | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `model_router.register_backend` | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER` | S13.2 §A    | Register a new `ModelBackendKind` adapter (Ollama, vLLM, vault-brokered external provider) into the router's precedence table; mediated through the S11.1 install pipeline — not a free-form write API. |
| `model_router.retire_backend`   | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER` | S13.2 §A    | Retire a registered backend; cascading effect closes any open circuit-breaker and removes the backend from precedence selection; emits `MODEL_BACKEND_RETIRED` evidence.                                |

#### W8.1.9 — S9.2 First-Boot Flow (1 action)

Queued by [S9.2 §11](../L1_Kernel_Bootstrap_Recovery/02_first_boot_flow.md). The recovery firstboot reset is the only path back to first-boot and is constitutionally recovery-mode-only.

| action_kind                | AdapterIOMode           | Permission      | Source spec | Purpose                                                                                                                                                                                                                                                                                                          |
| -------------------------- | ----------------------- | --------------- | ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `recovery.firstboot.reset` | `TYPED_PARAMETERS_ONLY` | `RECOVERY_ONLY` | S9.2 §11    | Operator-initiated reset of `/aios/system/firstboot/marker` from recovery mode with co-signer + explicit confirmation phrase; AI subjects rejected by `AISystemAdminBlocked` (INV-013); normal-mode subjects rejected by `RecoveryRequiredForSystemMutation` (INV-012); the cascade of guards is constitutional. |

### W8.2 Permission-class distribution

Truthful per-subsection recount:

| Subsection     | HUMAN_USER | AI_SUBJECT_OK | RECOVERY_ONLY | Subtotal |
| -------------- | ---------- | ------------- | ------------- | -------- |
| W8.1.1 (S12.1) | 1          | 3             | 0             | 4        |
| W8.1.2 (S9.3)  | 0          | 2             | 0             | 2        |
| W8.1.3 (S12.4) | 3          | 0             | 0             | 3        |
| W8.1.4 (S8.3)  | 5          | 0             | 1             | 6        |
| W8.1.5 (S8.4)  | 7          | 0             | 1             | 8        |
| W8.1.6 (S8.5)  | 1          | 0             | 0             | 1        |
| W8.1.7 (S11.3) | 1          | 4             | 0             | 5        |
| W8.1.8 (S13.2) | 2          | 0             | 0             | 2        |
| W8.1.9 (S9.2)  | 0          | 0             | 1             | 1        |
| **Totals**     | **20**     | **9**         | **3**         | **32**   |

`HUMAN_USER` required: **20 actions** (unchanged from pre-W9; the legacy `hardware.accept_drift` HUMAN_USER slot is filled by `hardware.accept_drift_accessory` — a rename, not an addition).
`AI_SUBJECT_OK`: **9 actions** (unchanged).
`RECOVERY_ONLY`: **3 actions** (was 2; W9 added `hardware.accept_drift_substrate` per Cluster 8 finding SIM-C-001).

#### W8.2.1 INV-002 enforcement check (mandatory)

Per [XX_Cross_Cutting/04 §4](../XX_Cross_Cutting/04_constitutional_meta_principles.md), every install / destructive / system-admin action that AI subjects could attempt to author MUST trigger the existing closed-enum reject (`APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` for app installs, or analogue codes per source) at the envelope FSM boundary. The Wave 8 catalog is audited as follows:

| action_kind                                                                     | INV-002 enforcement                                                                                                                                                                             | Reject code (closed)                                                        | Citation      |
| ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- | ------------- |
| `app.contribute_recipe`                                                         | AI envelope-FSM rejection on `policy_pending → executing`                                                                                                                                       | `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` (analogue)                        | S12.1 §9.1    |
| `compat.contribute_profile_observation`                                         | Same envelope-FSM rejection; reuses the S12.1 evidence type                                                                                                                                     | `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` (reused per S12.4 §9.9)           | S12.4 §9.9    |
| `compat.import_profile_from_upstream`                                           | Same envelope-FSM rejection                                                                                                                                                                     | `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` (reused)                          | S12.4 §9.9    |
| `compat.review_outlier_contribution`                                            | Same envelope-FSM rejection                                                                                                                                                                     | `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` (reused)                          | S12.4 §9.9    |
| `hardware.approve_removable_device` (and four siblings)                         | S2.3 hard-deny `AISystemAdminBlocked` at envelope validation                                                                                                                                    | `AI_REMOVABLE_DEVICE_BLOCKED` FOREVER                                       | S8.3 §8.3     |
| `network.resolver.set_backend` (and four resolver/VPN/mDNS HUMAN_USER siblings) | S2.3 hard-deny `AISystemAdminBlocked` at envelope validation; also covered by INV-002 §4.2 site 3 (Network AI_VAULT_BROKERED_ONLY) at the network-policy plane                                  | `AI_DIRECT_INTERNET_DENIED` FOREVER                                         | S8.4 §3.4     |
| `network.resolver.set_allowlist`                                                | RECOVERY_ONLY — both INV-002 (AI-author) and INV-012 (non-recovery) hard-denies cascade                                                                                                         | `RecoveryRequiredForSystemMutation` + `AISystemAdminBlocked`                | S8.4 §6.2     |
| `firmware.update.request`                                                       | S2.3 hard-deny `AISystemAdminBlocked` with rule id `hd.firmware_update.ai_authored`                                                                                                             | (S2.3 hard-deny class)                                                      | S8.5 §8       |
| `bridge.oci.fetch_explicit`                                                     | Subject is HUMAN_USER; the auto-bridge subjects (`is_ai = false`, `_system:bridge:*`) are not AI subjects, so INV-002 §4.2 sites 1+3 (vault, network) bind at their respective layers, not here | (n/a — not an AI subject)                                                   | S11.3 §145    |
| `model_router.register_backend` / `retire_backend`                              | Mediated through S11.1 install pipeline (INV-002 §4.2 site 2 — package install gate)                                                                                                            | `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` (reused via the install pipeline) | S13.2 §A note |
| `recovery.firstboot.reset`                                                      | INV-013 + INV-012 cascade hard-denies as documented in S9.2 §11.2 worked example                                                                                                                | `AISystemAdminBlocked` + `RecoveryRequiredForSystemMutation`                | S9.2 §11      |

Observation: every HUMAN_USER and RECOVERY_ONLY action in this Wave is covered by an existing INV-002 enforcement site (or its INV-012/INV-013 sibling). No new closed-enum reject code is introduced by this consolidation; existing FOREVER evidence semantics are reused. The §4.2 enforcement map (site 2 — Package install gate; site 4 — Capability Runtime queue cap) is the one that fires for `app.*`, `compat.*`, `model_router.*` AI-author attempts; sites 1 (vault) and 3 (network) cover the secret-bearing and outbound paths respectively.

The `AI_SUBJECT_OK` actions (9 total — three `app.*` Phase A/B/D, two `kernel.*` system-service, four `bridge.*` system-bridge) are explicitly designed for non-human subjects: they are either propose-only (INV-002-shaped — execution is the proposal record, not the side effect) or system-service identities under standing approval. None of them performs a destructive system mutation against `/aios/system/...` outside the recovery boundary.

### W8.3 Reconciliation (truthful arithmetic)

Total Wave 8 typed-action additions (post-W9 split): **32**.

This sub-spec (S10.1) had no enumerated typed-action catalog before Wave 8; the action_kind vocabulary was previously implicit and consumed via the L5 capability catalog (S1.1 §6.4). Wave 8 established the first explicit cross-spec catalog under this contract — at Wave 8 close the count was **31**. Wave 9 (Cluster 8 constitutional surgery, finding SIM-C-001 / CONS-S10-003) split `hardware.accept_drift` into `hardware.accept_drift_accessory` (rename of the original HUMAN_USER slot — not a count change) and `hardware.accept_drift_substrate` (new RECOVERY_ONLY entry — +1). After W9 the cumulative narrative total of S10.1-bound typed action_kinds explicitly listed in this sub-spec is **32**.

Note: this count is **not** the total system-wide action_kind count. Other action_kinds (e.g. `pkg.install`, `kernel.promote_to_a`, `kernel.module_set.add`, `policy_bundle.replace`, `app.launch`, the per-runtime `runtime.<adapter>.launch_app` family from S12.3, `RotateResolverList` and other internal RPCs that map to action_kinds) exist and bind to the same vocabulary but were either pre-existing (referenced in §16 worked examples) or were not explicitly queued by their source contract for S10.1 catalog consolidation — see W8.4 below.

### W8.4 Cross-spec impact note

#### W8.4.1 RecordType and verification primitive consolidation already complete

Each of the 31 Wave 8 actions emits at least one `RecordType` (action lifecycle records — `ACTION_REGISTERED`, `EXECUTION_OBSERVED`, `EXECUTION_VERIFICATION_FAILED`, etc. — plus the per-source-spec evidence types catalogued in S12.1 §13, S9.3 §15, S12.4 §11, S8.3 §12, S8.4 §10, S8.5 §11, S11.3 §10, S13.2 §11, S9.2 §10). All of these have been consolidated into S3.1 in earlier Wave passes. This Wave does not re-declare RecordType vocabulary.

#### W8.4.2 S2.4 property assertions held for separate refinement

S2.4 verification grammar may want a property assertion per typed action (e.g. `ACTION_DISPATCH_KIND_INTACT`, `ACTION_PERMISSION_CLASS_INTACT`) so an auditor can verify the runtime did not silently lower the permission class for a registered action. This is **not** consolidated in Wave 8 — it is a property-vocabulary extension that belongs in a future S2.4 refinement (queued).

#### W8.4.3 Source specs that queued no typed action for Wave 8

The following Tier 2 specs were inspected for queued S10.1 actions and found to introduce **no** new action_kinds requiring catalog consolidation:

- **S12.2 Package Model**: introduces on-disk `PackageObjectKind` / `PackageObjectState` / `RollbackKind` enums and references `aios.package.install`, `aios.package.update.stage`, `aios.package.update.promote`, `aios.package.rollback` action_kinds in worked examples (§5, §13), but those action_kinds are owned by S11.1 (the install pipeline contract) and were not explicitly queued by S12.2 for consolidation here. They remain in the implicit catalog under L5/S1.1 §6.4.
- **S12.3 Compatibility Runtime**: introduces the abstract `runtime.<adapter_name>.launch_app` family of per-`EcosystemRuntime` typed actions (concrete instances `runtime.linux_native.launch_app`, `runtime.proton.launch_app`, `runtime.waydroid.launch_app`, `runtime.kvm.launch_app`, etc., one per closed `EcosystemRuntime` value from S12.1 §3.1), but the contract explicitly states these are adapter-implementation contracts under S10.1 internal dispatch (§5.6 of S12.3) and are **not** queued for catalog consolidation here. They are reachable only via the orchestrator-owned `app.launch` action.
- **S7.6 CLI Renderer**: introduces no new typed actions; the CLI is a renderer over the existing typed-action surface (§1 of S7.6 explicitly forbids free-form shell escapes per `AdapterIOMode`).
- **S11.2 Marketplace**: introduces the `DiscoveryProposal` action envelope shape (§9 of S11.2) but defers the full schema to L5; not consolidated here. Install actions on the marketplace surface flow through the existing S11.1 install pipeline.
- **S14.1 Failure Handling**: consumer of S10.1; introduces no new typed actions. Adapter degradation (row 19 of §6) is a state mutation owned by S10.1 §3.4 (`AdapterStability` transitions), not a separate typed action.
- **S14.2 Telemetry Pipeline**: consumer of S10.1; uses `adapter_kind` as a label source. No new typed actions.

#### W8.4.4 Audit findings (potential RECOVERY_ONLY mismatches)

This consolidation surfaces two action_kinds whose permission classification merits a follow-up audit pass against the L0 invariant catalog:

- **`hardware.rebind_driver`** (W8.1.4) is classified `HUMAN_USER` here, matching S8.3 §5.3 + §8.3. However, S8.3 §9.6 documents that out-of-tree driver bind requires `RECOVERY_REQUIRED` per INV-012, and the action's adapter cannot statically distinguish between in-tree rebind (legitimately HUMAN_USER) and out-of-tree rebind (requires recovery) at envelope validation time — the discrimination is inside the adapter's driver-binding evaluator. An auditor reading W8.1.4 in isolation might miss this nuance. Recommended follow-up: introduce a closed `target.driver_provenance` condition field at S2.3 (already queued in S8.3 §11.1) and split this into `hardware.rebind_driver_in_tree` (HUMAN_USER) versus `hardware.rebind_driver_out_of_tree` (RECOVERY_ONLY) in the next refinement Wave.
- **`hardware.accept_drift`** (W8.1.4) was classified `HUMAN_USER` at Wave 8. A drift entry that involves a quarantined-device firmware downgrade or a CPU/microcode/TPM substitution may merit RECOVERY_ONLY treatment because the substrate is constitutional (cf. S8.5 §9.3 constitutional refusal scopes). A stricter reading of INV-022 (recovery aesthetic distinct, by analogy) and INV-012 (recovery required for system mutation) requires recovery-mode for the constitutional-scope subset. **Resolved in Wave 9** — the action was split into `hardware.accept_drift_accessory` (HUMAN_USER) and `hardware.accept_drift_substrate` (RECOVERY_ONLY); see §W9.1 below and the W8.1.4 table above.

Both observations are flagged as **audit findings**, not blockers. The current classifications match the source specs verbatim; tightening to RECOVERY_ONLY is a constitutional decision that requires a deliberate L0 invariant promotion (per the DEC-025 / DEC-026 discipline), not a unilateral edit in this consolidation pass.

#### W8.4.5 No execution-discipline change

This Wave is purely additive against §3 (vocabulary) and the L5 capability catalog. It does not modify §4 (lifecycle FSM), §5 (gRPC service surface), §6 (eight-step pre-dispatch), §7 (verification and rollback), §8 (emergency override), §10 (adapter manifest contract), or §11 (queueing and AI-share cap). Every Wave 8 action is dispatched under the existing FSM and the existing `ISOLATED_SANDBOX` discipline; no new dispatch kind, no new failure reason, no new error code, no new queue class is introduced.

### W9 amendments (Wave 9 constitutional surgery)

#### W9.1 Hardware-drift accept-side split (Cluster 8, finding SIM-C-001 / CONS-S10-003)

Wave 8's audit candidate W8.4.4 finding B6 was deferred pending an L0 invariant-promotion decision. Wave 9 makes that decision: hardware-graph drift acceptance is split along the constitutional-substrate axis.

The legacy entry `hardware.accept_drift` is removed from the W8.1.4 catalog and replaced by:

- `hardware.accept_drift_accessory` (TYPED_PARAMETERS_ONLY, HUMAN_USER) — accessory hardware drift acceptance under normal-mode HUMAN_USER consent. Covers ALL `DeviceClass` values EXCEPT the constitutional substrate set.
- `hardware.accept_drift_substrate` (TYPED_PARAMETERS_ONLY, RECOVERY_ONLY) — substrate hardware drift acceptance under recovery-mode + S5.4 hardware-key co-signer discipline. Covers `DeviceClass` ∈ {`CPU`, `TPM_2_0`, `BIOS_UEFI`, `GPU_DISCRETE_FIRMWARE_BOUND`}.

The arithmetic delta is +1 to RECOVERY_ONLY (now 3) and +1 to total Wave-8 catalog size (now 32). HUMAN_USER count is unchanged (the rename `accept_drift → accept_drift_accessory` is a slot replacement).

The split formalises the W8.4.4 audit candidate finding B6 and resolves it. The S2.3 condition field `target.is_constitutional_substrate: bool` (derived from `device_class IN {CPU, TPM_2_0, BIOS_UEFI, GPU_DISCRETE_FIRMWARE_BOUND}`) is added by W9-A in S2.3 §26 alongside the new hard-deny rule `ConstitutionalSubstrateRequiresRecovery` that fires when `hardware.accept_drift_substrate` is dispatched outside `request.environment = RECOVERY`.

A new closed-vocabulary RecordType is queued for S3.1 W10:

- `HARDWARE_SUBSTRATE_ACCEPT_OUTSIDE_RECOVERY_BLOCKED` (FOREVER) — emitted when a `hardware.accept_drift_substrate` envelope is dispatched outside recovery mode and refused by `ConstitutionalSubstrateRequiresRecovery`. Carries the AI-or-non-AI subject id, the originating drift record id, the device set, and the action envelope id. Pairs with the resolution-side `HARDWARE_GRAPH_DRIFT_ACCEPTED` record (S8.3 §12, also queued for S3.1 W10) for full audit-chain completeness.

W9.1 does not modify the L3 lifecycle FSM, the gRPC service surface, the eight-step pre-dispatch, verification, rollback, queue, or AI-share cap. It is a vocabulary delta on the typed-action catalog plus the queued S2.3 + S3.1 cross-spec touch-ups described above.

> **Forward reference (W10):** the W8.3 cumulative narrative total of **32** is superseded by W10 below, which consolidates 9 additional typed actions explicitly queued for S10.1 by Tier 2 sources S15.1 (Unit Manifest) and S15.3 (Adapter Model). The post-W10 cumulative total is **41**. The W8.4.3 "no contribution" list omitted S15.1 and S15.3; that omission is also closed by W10.

## Wave 10 cross-spec touch-up (Cluster 7 — S15.1 + S15.3 missing actions)

Applied 2026-05-11. Sources: [S15.1 §13 cross-spec dependencies row for S10.1](./01_unit_manifest.md), [S15.3 §3.2 / §6.2 / §7 / §8 adapter lifecycle](./04_adapter_model.md). This Wave consolidates the typed action_kinds explicitly declared by S15.1 and S15.3 as **producers to S10.1** that were missed by Wave 8's first-pass cross-spec sweep (Cluster 7 findings CONS-S10-001 from S15.1 line 732 and CONS-S10-002 from S15.3 §6.2 / §7 / §8). It is **additive**: no existing §3 enum, §4 lifecycle, §5 gRPC surface, §10 manifest contract, or §11 queue discipline is redefined. Every action below binds to the existing closed `ActionDispatchKind`, `AdapterIOMode`, and `AdapterStability` vocabulary.

### W10.1 From S15.1 Unit Manifest (5 typed actions — finding CONS-S10-001)

Queued by [S15.1 §13](./01_unit_manifest.md) (the cross-spec dependencies row for S10.1: "unit-level action kinds (`unit.start`, `unit.stop`, `unit.restart`, `unit.health_probe`, `unit.rollback`) referenced by name; full per-action target schemas live in `04_adapter_model.md`"). The five actions form the per-unit dispatch surface used by the SGR-side state machine in S15.1 §4 and the rollback discipline in S15.1 §9. The SGR is itself a system-service subject (`_system:service:sgr`, `is_ai = false`), but the canonical _author_ of `unit.start` / `unit.stop` / `unit.restart` / `unit.rollback` is the operator submitting a desired-state mutation; `unit.health_probe` is internally dispatched by the SGR as a read-only observation under standing approval, hence `AI_SUBJECT_OK` (system-service class — not AI-origin in the INV-002 sense, but admitted under the same propose-only / standing-approval discipline).

| action_kind         | AdapterIOMode           | Permission      | RecordType emissions (S15.1 §10)                                                | Source spec           | Purpose                                                                                                                                                                                                                                                                                                                                                          |
| ------------------- | ----------------------- | --------------- | ------------------------------------------------------------------------------- | --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `unit.start`        | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | `UNIT_STARTED` (STANDARD_24M); `UNIT_FAILED` (EXTENDED_60M) on adapter failure  | S15.1 §4.2 + §13      | Operator-initiated SGR unit start; adapter performs the unit's start protocol; transition `STARTING → RUNNING` (S15.1 §4 AT for `STARTING`); failure transitions `STARTING → FAILED` with `ExecutionFailureReason` carried per S10.1 §3.6.                                                                                                                       |
| `unit.stop`         | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | `UNIT_STOPPED` (STANDARD_24M); `UNIT_FAILED` (EXTENDED_60M) on adapter failure  | S15.1 §4.2 + §13      | Operator-initiated graceful shutdown; transition `STOPPING → STOPPED` on success, `STOPPING → FAILED` on adapter error; carries closed `stop_reason ∈ {OPERATOR, DEPENDENCY_STOP, ROLLBACK, RETIREMENT}`.                                                                                                                                                        |
| `unit.restart`      | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | `UNIT_STOPPED` then `UNIT_STARTED` (or `UNIT_FAILED` on either leg)             | S15.1 §4.2 + §6 + §13 | Operator-initiated restart; sequenced as `unit.stop` then `unit.start` against the same adapter snapshot; restart-policy-driven internal restarts use the same adapter contract but are SGR-authored and dispatched under the standing restart-policy approval.                                                                                                  |
| `unit.health_probe` | `TYPED_PARAMETERS_ONLY` | `AI_SUBJECT_OK` | `UNIT_HEALTHY` (STANDARD_24M); `UNIT_DEGRADED` (EXTENDED_60M) on probe warning  | S15.1 §6 + §13        | Read-only health observation dispatched by the SGR under the closed `HealthCheckKind` (HTTP_OK / TCP_PORT / UNIX_SOCKET / FILE_PRESENT / COMMAND_EXIT_ZERO); no state mutation other than the health-state transition; subject is `_system:service:sgr`.                                                                                                         |
| `unit.rollback`     | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER`    | `UNIT_ROLLBACK_TRIGGERED` (EXTENDED_60M); `UNIT_FAILED` if CAS or adapter fails | S15.1 §9 + §13        | Operator-initiated (or `RollbackTrigger`-driven, internally dispatched by the SGR) S1.3 `PromotePointer` CAS that rolls a unit's `rollback_pointer` back to `expected_current_version_id`; CAS failure emits `UNIT_ROLLBACK_TRIGGERED` with `RollbackOutcome = CAS_FAILED` (S10.1 §3.7). Operator-authored variant requires explicit S5.3 EXACT_ACTION approval. |

All five actions dispatch as `ISOLATED_SANDBOX` per §3.2 (system-service authorship targeting `/aios/...` units). Default `AdapterStability` is the operating stability of the unit's bound adapter — manifest-declared maximum subject to operator promotion via `runtime.adapter.set_stability` (W10.2 below).

### W10.2 From S15.3 Adapter Model (4 typed actions — finding CONS-S10-002)

Queued by [S15.3 §6.2 / §7 / §8](./04_adapter_model.md) (registration discipline at §6.2.5, deregistration at §7.3, retirement at §7.5, stability promotion at §3.2 and §8). All four are operator-class typed actions; AI subjects are hard-denied at envelope validation by the existing S2.3 hard-deny `AISystemAdminBlocked` (INV-013 — adapter-directory mutation is constitutional admin surface).

The fourth row, `runtime.adapter.set_stability`, is **already referenced in S10.1 §3.4 (line 165)** as the operator-only mechanism by which `AdapterStability` transitions occur. Wave 8 did not catalog it because §3.4 declared the mechanism narratively; W10.2 now consolidates it into the explicit cross-spec typed-action catalog so the closed-vocabulary discipline (§3 of this sub-spec) holds end-to-end. **No double-add** — §3.4 retains its narrative reference; this catalog row is the canonical action_kind entry.

| action_kind                     | AdapterIOMode           | Permission   | RecordType emissions (S15.3 §9)                                                                                                                                     | Source spec                  | Purpose                                                                                                                                                                                                                                                                                              |
| ------------------------------- | ----------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `runtime.adapter.register`      | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER` | `ADAPTER_REGISTRATION_REQUESTED` (STANDARD_24M); `ADAPTER_REGISTERED` on success or `ADAPTER_REGISTRATION_REJECTED` (FOREVER) on any §6.2.1–§6.2.6 failure          | S15.3 §3.2 + §6.2 + §9       | Operator-class registration of a new adapter manifest into the directory. AI-origin attempts hard-denied at envelope validation per S15.3 §6.2 (INV-013). Idempotent on `(adapter_id, adapter_version, manifest_digest)`; same-version different-digest rejected with `MANIFEST_DIGEST_MISMATCH`.    |
| `runtime.adapter.deregister`    | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER` | `ADAPTER_DEREGISTERED` (EXTENDED_60M) with `reason ∈ {VOLUNTARY, OPERATOR}`                                                                                         | S15.3 §7.3 + §7.4 + §9       | Operator-class voluntary or incident-response withdrawal of an adapter; transition `REGISTERED → DEREGISTERED` (AT9). In-flight actions drain per §7.1.                                                                                                                                              |
| `runtime.adapter.retire`        | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER` | `ADAPTER_DEREGISTERED` (EXTENDED_60M) with `reason = OPERATOR_RETIRE`; cascades `ADAPTER_REGISTRATION_REJECTED` retroactively if linked to constitutional violation | S15.3 §7.5 + §9              | Operator-class graceful retirement (terminal `RETIRED` state, AT12); typically used as a security-incident response or constitutional-violation forced retirement; refuses all subsequent dispatches.                                                                                                |
| `runtime.adapter.set_stability` | `TYPED_PARAMETERS_ONLY` | `HUMAN_USER` | (No new RecordType in S15.3 §9; the stability transition is recorded as part of the action lifecycle records ACTION_REGISTERED/EXECUTION_OBSERVED on this kind)     | S10.1 §3.4 + S15.3 §3.2 + §8 | Operator-class promotion or demotion across the closed `AdapterStability` enum (`REGISTERED` ↔ `EXPERIMENTAL` ↔ `STABLE` ↔ `DEPRECATED`; `RETIRED` is terminal and reachable only via `runtime.adapter.retire` or AT12). Already referenced in S10.1 §3.4; W10.2 catalogs it as a typed action_kind. |

All four actions dispatch as `ISOLATED_SANDBOX` per §3.2 (operator authorship targeting `/aios/system/runtime/adapters/...`). The composer floor for the adapter-directory surface is the runtime safety floor per S3.2 §5.4.

### W10.3 Reconciliation (truthful arithmetic)

| Anchor                                    | Count  |
| ----------------------------------------- | ------ |
| W8 cumulative (post-W9 split)             | **32** |
| W10.1 additions (S15.1 unit lifecycle)    | **+5** |
| W10.2 additions (S15.3 adapter lifecycle) | **+4** |
| **W10 cumulative**                        | **41** |

Updated permission-class distribution (cumulative across W8 + W9 + W10):

| Subsection                      | HUMAN_USER | AI_SUBJECT_OK | RECOVERY_ONLY | Subtotal |
| ------------------------------- | ---------- | ------------- | ------------- | -------- |
| W8.1.1 (S12.1)                  | 1          | 3             | 0             | 4        |
| W8.1.2 (S9.3)                   | 0          | 2             | 0             | 2        |
| W8.1.3 (S12.4)                  | 3          | 0             | 0             | 3        |
| W8.1.4 (S8.3, post-W9)          | 5          | 0             | 1             | 6        |
| W8.1.5 (S8.4)                   | 7          | 0             | 1             | 8        |
| W8.1.6 (S8.5)                   | 1          | 0             | 0             | 1        |
| W8.1.7 (S11.3)                  | 1          | 4             | 0             | 5        |
| W8.1.8 (S13.2)                  | 2          | 0             | 0             | 2        |
| W8.1.9 (S9.2)                   | 0          | 0             | 1             | 1        |
| W10.1 (S15.1 unit lifecycle)    | 4          | 1             | 0             | 5        |
| W10.2 (S15.3 adapter lifecycle) | 4          | 0             | 0             | 4        |
| **Totals (post-W10)**           | **28**     | **10**        | **3**         | **41**   |

`HUMAN_USER`: **28 actions** (was 20 post-W9; +8 from W10 — 4 unit-lifecycle operator actions + 4 adapter-lifecycle operator actions).
`AI_SUBJECT_OK`: **10 actions** (was 9 post-W9; +1 from W10 — `unit.health_probe` system-service dispatch).
`RECOVERY_ONLY`: **3 actions** (unchanged — W10 introduces no new recovery-mode-only actions).

#### W10.3.1 INV-002 enforcement check

Every W10 action that AI subjects could attempt to author MUST trigger an existing closed-enum reject at the envelope FSM boundary:

| action_kind                                                   | INV-002 / INV-013 enforcement                                                                         | Reject code (closed)                          | Citation                   |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- | --------------------------------------------- | -------------------------- |
| `unit.start` / `unit.stop` / `unit.restart` / `unit.rollback` | S2.3 hard-deny `AISystemAdminBlocked` for AI-origin authorship attempting unit-state mutation         | `AISystemAdminBlocked` (S2.3 hard-deny class) | S15.1 §13 (L0 row INV-002) |
| `unit.health_probe`                                           | Read-only; dispatched only by `_system:service:sgr` system-service subject under standing approval    | (n/a — read-only system-service)              | S15.1 §6                   |
| `runtime.adapter.register` / `deregister` / `retire`          | S2.3 hard-deny `AISystemAdminBlocked` for AI subjects (S15.3 §6.2.5 — registration is operator-class) | `AISystemAdminBlocked`                        | S15.3 §6.2 + §7            |
| `runtime.adapter.set_stability`                               | Same envelope-FSM rejection — adapter-directory mutation is constitutional admin surface              | `AISystemAdminBlocked`                        | S10.1 §3.4 + S15.3 §3.2    |

Observation: every HUMAN_USER action introduced by W10 is covered by the existing INV-013 hard-deny `AISystemAdminBlocked` at S2.3 envelope validation. No new closed-enum reject code is introduced. The single `AI_SUBJECT_OK` action (`unit.health_probe`) is read-only and authored by the SGR system-service subject — it is not an AI-origin action in the INV-002 sense.

### W10.4 Closing W8.4 audit candidates and "no contribution" omissions

#### W10.4.1 W8.4.4 audit candidate B6 — status

Audit candidate B6 (the `hardware.accept_drift` constitutional-substrate split) was **resolved by Wave 9** (see §W9.1). W10 has no further action on this finding; it is recorded here only to confirm closure for the cross-spec audit trail.

#### W10.4.2 W8.4.3 "no contribution" omission — closure

Wave 8's §W8.4.3 enumerated Tier 2 specs that "queued no typed action for Wave 8" and listed S12.2, S12.3, S7.6, S11.2, S14.1, S14.2. **It omitted S15.1 and S15.3.** That omission was the root cause of Cluster 7 findings CONS-S10-001 (S15.1 — 5 actions) and CONS-S10-002 (S15.3 — 4 actions). W10.1 + W10.2 close the omission by consolidating both source specs into the explicit catalog. The corrected reading of §W8.4.3 is: of the seven Tier 2 specs reachable from this sub-spec's cross-spec footprint, S15.1 and S15.3 **did** queue typed actions for S10.1 — they are now consolidated in W10.

### W10.5 Cross-spec impact

#### W10.5.1 RecordTypes referenced

The eight S15.1 evidence record types (`UNIT_REGISTERED`, `UNIT_STARTED`, `UNIT_HEALTHY`, `UNIT_DEGRADED`, `UNIT_FAILED`, `UNIT_STOPPED`, `UNIT_ROLLBACK_TRIGGERED`, `UNIT_DEPENDENCY_CYCLE_DETECTED`) and the ten S15.3 evidence record types (`ADAPTER_REGISTRATION_REQUESTED`, `ADAPTER_REGISTRATION_REJECTED`, `ADAPTER_REGISTERED`, `ADAPTER_HEALTHY`, `ADAPTER_DEGRADED`, `ADAPTER_ACTION_KIND_VIOLATION`, `ADAPTER_CAPABILITY_VIOLATION`, `ADAPTER_HOT_RELOADED`, `ADAPTER_DOWNGRADE_REJECTED`, `ADAPTER_DEREGISTERED`) referenced by the W10 typed-action catalog rows are either already present in S3.1's closed `RecordType` enum or are queued for absorption by S3.1 W10 (handled separately by Wave 10-A). This sub-spec does not redefine RecordType vocabulary.

#### W10.5.2 S2.4 verification properties

S2.4 property assertions for the W10 actions (e.g. `UNIT_DISPATCH_KIND_INTACT`, `ADAPTER_LIFECYCLE_PERMISSION_INTACT`) are out of scope here — they belong to the W8.4.2 deferred S2.4 property-vocabulary extension, which is owned by Wave 10-B (the S2.4 refinement track). W10 makes no S2.4 commitment beyond what W8 already deferred.

#### W10.5.3 No execution-discipline change

Identically to W8 and W9, this Wave is purely additive against §3 (vocabulary) and the L5 capability catalog. It does not modify §4 (lifecycle FSM), §5 (gRPC service surface), §6 (eight-step pre-dispatch), §7 (verification and rollback), §8 (emergency override), §10 (adapter manifest contract), or §11 (queueing and AI-share cap). Every W10 action is dispatched under the existing FSM and the existing `ISOLATED_SANDBOX` discipline; no new dispatch kind, no new failure reason, no new error code, no new queue class is introduced.

#### W10.5.4 Out of scope (handled by other Wave 10 work tracks)

- **S3.1 RecordType absorption** — owned by Wave 10-A. The S15.1 eight-record set, the S15.3 ten-record set, the W9-queued `HARDWARE_SUBSTRATE_ACCEPT_OUTSIDE_RECOVERY_BLOCKED`, and the W9-queued `HARDWARE_GRAPH_DRIFT_ACCEPTED` are all consolidated there.
- **S2.4 property extensions** — owned by Wave 10-B.
- **S9.1 enum touch-ups** — owned by Wave 10-D.
- **S13.1 cross-references** — owned by Wave 10-E.

## See also

- [L3 Overview](./00_overview.md)
- [L3 Unit Manifest](./01_unit_manifest.md) (deferred)
- [L3 State Transitions](./02_state_transitions.md) (deferred)
- [L3 Adapter Model](./04_adapter_model.md) (deferred)
- [S0.1 Action Envelope + Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S2.3 Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S5.3 Approval Mechanics](../L4_Policy_Identity_Vault/04_approval_mechanics.md)
- [S5.4 Emergency Override](../L4_Policy_Identity_Vault/05_emergency_override.md)
- [S2.4 Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [S3.1 Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S3.2 Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [L0.4 Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md)
- [Rev.1 §10 — AIOS-SGR](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.1 §13 — Typed Actions and Capability Runtime](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
