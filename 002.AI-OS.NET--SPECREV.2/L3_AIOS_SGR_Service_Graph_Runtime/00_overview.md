# L3 — AIOS-SGR (Service Graph Runtime) and Capability Runtime

Status: `PARTIAL` (all four sub-specs `CONTRACT`; layer headline remains PARTIAL until E2+ implementation evidence)

## Responsibility

AIOS-SGR owns desired and runtime machine state — services, one-shot jobs, timers, mounts, devices, app sessions, agent workers, model servers, health checks, rollback, resource limits, sandbox profiles, approval gates. The Capability Runtime executes typed actions emitted by the Cognitive Core, after policy approval.

## Layer invariants (from Rev.1 §6, §10, §13)

- Runtime transitions require a policy decision.
- Runtime correctness must not depend on LLM availability.
- L3 may ask L4 policy, but must not bypass it.
- `ExecuteAction` accepts only approved actions; expired approvals are rejected.
- Adapters must not accept free-form shell commands as primary input.
- Unsupported actions fail closed.

## Dependencies

May depend on: L0, L1, L2.

## Planned sub-specs

| File                            | Topic                                                                | Status     | Phase |
| ------------------------------- | -------------------------------------------------------------------- | ---------- | ----- |
| `01_unit_manifest.md`           | Service unit schema, sandbox profile, verification, rollback pointer | `CONTRACT` | S15.1 |
| `02_state_transitions.md`       | Desired-state graph evaluation, dependency solve, A/B promotion      | `CONTRACT` | S15.2 |
| `03_capability_runtime_grpc.md` | gRPC service contract (`ValidateAction`/`ExecuteAction`/...)         | `CONTRACT` | S10.1 |
| `04_adapter_model.md`           | Adapter manifest, capability registration, fail-closed semantics     | `CONTRACT` | S15.3 |

## Cross-cutting contract dependency

L3 consumes the [Action Envelope + Lifecycle contract](../XX_Cross_Cutting/01_action_envelope_lifecycle.md) (S0.1) — this is the wire format and lifecycle for everything that flows through `ExecuteAction`.

L3 also consumes the [ProxGuard Reference Model notes](../XX_Cross_Cutting/02_proxguard_reference_model.md) as a prototype donor for manifest-driven simulation, runtime adapters, sealed package handoff, isolated executor inboxes, and golden path acceptance tests.

If ProxGuard is installed as an AIOS app, L3 may treat it as an optional capability provider for service simulation, deployment, restart, status, DNS plan/apply, gateway routing, and audit read operations. ProxGuard remains outside the core runtime and must pass through AIOS action envelopes, policy, sandboxing, verification, and evidence.

## See also

- [Rev.1 §10 — AIOS-SGR](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.1 §13 — Typed Actions and Capability Runtime](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
