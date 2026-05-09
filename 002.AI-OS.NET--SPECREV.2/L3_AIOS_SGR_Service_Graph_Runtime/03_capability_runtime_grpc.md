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
