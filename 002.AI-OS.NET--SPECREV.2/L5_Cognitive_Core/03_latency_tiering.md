# Latency Tiering (Rev.2)

| Field     | Value                                                        |
| --------- | ------------------------------------------------------------ |
| Status    | `CONTRACT` draft                                             |
| Phase tag | S1.2                                                         |
| Layer     | L5 Cognitive Core                                            |
| Consumes  | S1.1 Capability Translator, S0.1 Action Envelope, L0 status  |
| Produces  | Routing decisions for direct, local-model, and external paths |

## 1. Purpose

Latency Tiering defines when AIOS should use deterministic code, a small local model, a powerful local model, or an external model. It prevents every user action from becoming an expensive LLM round trip.

The goal is not maximum intelligence on every request. The goal is the smallest cognition path that can safely produce a typed, verifiable result.

## 2. Core invariant

No tier may bypass typed actions, policy checks, verification, or evidence.

```text
fast path != unsafe path
slow path != more privileged path
```

The tier only decides how cognition happens before an action draft exists.

## 3. Tiers

| Tier | Name                  | Typical use                                  | Model use        | Output                                |
| ---- | --------------------- | -------------------------------------------- | ---------------- | ------------------------------------- |
| T0   | Cached UI/state path  | open recent project, show status, recall view | none             | UI state or read-only query           |
| T1   | Deterministic path    | `restart nginx`, `install docker`             | none             | one action draft from exact rules     |
| T2   | Catalog retrieval     | known action, fuzzy wording                   | optional rerank  | one or more action drafts             |
| T3   | Local cognitive path  | ambiguous goal, target extraction             | local model      | clarification or action drafts        |
| T4   | Powerful reasoning    | multi-step goal, novel workflow               | powerful/external | plan plus action drafts               |

T0 and T1 must work without external AI. Boot, recovery, and basic administration depend on these paths.

## 4. Routing inputs

The router evaluates:

- user utterance
- active renderer and interaction mode
- requested operation type
- risk flags estimated by S1.1
- capability catalog match confidence
- target schema completeness
- local model availability
- external model policy
- privacy classification of context
- current system pressure
- user's explicit preference

## 5. Routing rules

| Condition                                      | Route |
| ---------------------------------------------- | ----- |
| Exact known command with complete target        | T1    |
| Known capability, fuzzy phrase, complete target | T2    |
| Missing target field                           | T3 clarification |
| Multiple close capability matches              | T3 clarification |
| Multi-action operational goal                  | T4 planner path |
| Secret-bearing context                         | T0-T3 only, no external model |
| Recovery mode                                  | T0-T1 only |
| External AI unavailable                        | highest available local tier |
| Policy forbids model egress                    | local tiers only |

If routing confidence is low, the system must choose clarification over execution.

## 6. Direct path

The direct path is a deterministic compiler from common utterance patterns to capability translator requests.

Examples:

| Input                   | Direct action                    |
| ----------------------- | -------------------------------- |
| `restart nginx`         | `service.restart {service:nginx}` |
| `status docker`         | `service.status {service:docker}` |
| `install docker`        | `package.install {package:docker}` |
| `open latest project x` | read-only AIOS-FS view query      |

Direct path still emits translation evidence. It records `model_ids=[]`.

## 7. Local model path

The local model path is used when language parsing is needed but private context should not leave the machine.

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

## 8. Powerful reasoning path

T4 is reserved for goals that require planning, decomposition, or unfamiliar context.

T4 may produce:

- an intent object
- a plan object
- ordered calls to S1.1 translation
- clarification questions
- explanation

T4 may not produce executable side effects directly. Each plan step still becomes an S0.1 action envelope through S1.1.

## 9. Degraded mode

AIOS must continue useful operation when high tiers are unavailable.

| Failure                       | Required degradation                                  |
| ----------------------------- | ----------------------------------------------------- |
| external model unavailable    | use local tiers; mark model route degraded            |
| local model unavailable       | use deterministic paths; require exact typed commands |
| vector index unavailable      | lexical/exact catalog search only                     |
| catalog unavailable           | block state-changing translation                      |
| recovery mode active          | T0/T1 only                                            |

Degraded routing is evidence-worthy. The user-facing renderer may show reduced cognition, but action safety is unchanged.

## 10. Evidence

Every routing decision records:

- routing decision id
- selected tier
- fallback tiers considered
- model ids used
- reason code
- privacy class
- catalog version
- linked intent id
- linked translation id

Prompt bodies are not stored by default. Prompt hashes and redacted context references are enough for normal evidence.

## 11. Acceptance criteria

- Exact low-risk commands route without an LLM.
- Ambiguous commands ask clarification.
- External AI can be disabled without breaking service/package/status operations.
- Secret-bearing context never routes to external models.
- Recovery mode blocks high cognition tiers.
- Every translation includes routing evidence.

