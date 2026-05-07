# Policy Kernel (Rev.2)

| Field     | Value                                      |
| --------- | ------------------------------------------ |
| Status    | `CONTRACT` draft                           |
| Phase tag | S2.3                                       |
| Layer     | L4 Policy, Identity, Vault                 |
| Consumes  | S0.1 Action Envelope, L2 object tags, L3 adapter metadata |
| Produces  | policy decisions, approval requirements, denials |

## 1. Purpose

The Policy Kernel is the operating constitution of AIOS. It decides whether a typed action may proceed, requires approval, or must be denied.

It evaluates typed action envelopes, not shell commands.

## 2. Decision model

```text
ActionEnvelope.request
  -> normalize subject
  -> enrich with object/resource metadata
  -> evaluate hard denies
  -> evaluate allow/approval rules
  -> bind sandbox and constraints
  -> emit policy decision
```

## 3. Decision result

```json
{
  "policy_decision_id": "poldec_...",
  "request_hash": "blake3:...",
  "decision": "allow | require_approval | deny",
  "reason": "ScopedAllow",
  "constraints": {
    "sandbox_profile_id": "host-service-control",
    "max_runtime_seconds": 30
  },
  "approval": {
    "required": true,
    "approval_scope": "exact_request_hash",
    "expires_at": "..."
  }
}
```

Approvals bind to exact request hash. If the request changes, approval is invalid.

## 4. Rule precedence

Order is fixed:

1. Invalid subject -> deny.
2. Hard deny -> deny.
3. Emergency override denylist -> deny.
4. Explicit scoped deny -> deny.
5. Explicit scoped allow -> allow or require approval.
6. Default -> deny.

Default deny is mandatory.

## 5. Hard denies

Hard-denied classes:

- raw secret read by AI subject
- recursive deletion of `/home`, `/root`, `/aios`, or recovery partitions
- policy log deletion
- evidence log mutation
- disabling Policy Kernel
- disabling recovery path
- modifying boot chain without dedicated recovery approval
- untyped shell execution as privileged subject

Emergency override cannot bypass evidence logging.

## 6. Policy language decision

Rev.2 uses a small AIOS policy schema as the canonical authoring format, with optional compilation to OPA/Rego or CEL later.

Rationale:

- AIOS needs request-hash-bound approvals and action-envelope-specific semantics.
- The policy surface is typed and smaller than general-purpose authorization.
- A canonical schema keeps renderer prompts and evidence stable.
- OPA/CEL can still be backend evaluators if they preserve canonical decisions.

## 7. Policy rule shape

```yaml
rule_id: allow_restart_user_services
effect: allow
subjects:
  - human:lucky
actions:
  - service.restart
conditions:
  environment: LOCAL
  target.service_in_group: user-managed
constraints:
  sandbox_profile_id: host-service-control
approval:
  required: false
```

## 8. Simulation

Policy evaluation must support simulation:

```text
Would this action be allowed, require approval, or be denied?
```

Simulation emits evidence marked simulated and never grants durable approval.

## 9. Approval mechanics boundary

This spec defines that approval is required and how it binds. Delivery is deferred to `04_approval_mechanics.md`.

Required properties:

- approval binds to exact request hash
- approval has TTL
- approval records subject and renderer
- approval result is evidence-linked
- approval cannot mutate the request

## 10. Acceptance criteria

- Default deny works.
- Hard denies override all allow rules.
- Request mutation invalidates approval.
- Policy simulation is possible.
- Policy decision links to action envelope and evidence.
- AI subjects cannot approve their own high-risk actions.

