# L5 — Cognitive Core

Status: `PARTIAL`

## Responsibility

The Cognitive Core is responsible for cognition, not direct execution. It hosts the Intent Engine, Semantic Context Engine, Planner / Orchestrator, Capability Translator, Policy Client, Verification Assistant, Persistent Memory, System Knowledge Graph, Agent Coordinator, Evidence Logger, and Model Router.

## Layer invariants (from Rev.1 §6, §15)

- L5 may propose actions, but must not execute them directly.
- The Cognitive Core must not approve its own high-risk actions.
- The Cognitive Core must not read raw secrets.
- The Cognitive Core must degrade if external AI is unavailable.
- Boot and recovery cannot depend on external AI.
- Model calls must exclude secrets.
- Model routing decisions must be logged.

## Dependencies

May depend on: L0, L1, L2, L3, L4.

## Planned sub-specs

| File                          | Topic                                                                                | Status  | Phase |
| ----------------------------- | ------------------------------------------------------------------------------------ | ------- | ----- |
| `01_intent_engine.md`         | Goal → intent object; risk hint extraction; clarification loops                      | `SHELL` | —     |
| `02_capability_translator.md` | LLM -> typed action mapping at scale (1000s of actions); RAG-over-capabilities        | `CONTRACT` | S1.1  |
| `03_latency_tiering.md`       | Direct (no-LLM) path vs cognitive path; routing rules; UX latency budget             | `CONTRACT` | S1.2  |
| `04_planner.md`               | Plan object schema; multi-step reasoning; replanning; partial failure recovery       | `SHELL` | —     |
| `05_model_router.md`          | Local default vs powerful local vs external; degradation rules; cost/latency budgets | `SHELL` | —     |
| `06_persistent_memory.md`     | Project, architectural decision, fix, failure, workflow, preference memories         | `SHELL` | —     |
| `07_agent_coordinator.md`     | Multi-agent task assignment; shared memory; arbitration                              | `SHELL` | —     |

## Cross-cutting contract dependency

L5 _produces_ envelopes that conform to the [Action Envelope + Lifecycle contract](../XX_Cross_Cutting/01_action_envelope_lifecycle.md) (S0.1). The first L5 contract is [Capability Translator](02_capability_translator.md) (S1.1).

## See also

- [Rev.1 §15 — Cognitive Core](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
