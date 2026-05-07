# Capability Translator (Rev.2)

| Field          | Value                                                                                     |
| -------------- | ----------------------------------------------------------------------------------------- |
| Status         | `CONTRACT` (drafted; awaiting review before design approval)                              |
| Phase tag      | S1.1                                                                                      |
| Layer          | L5 Cognitive Core                                                                         |
| Consumes       | L0 status taxonomy, L3 adapter manifests, L4 subject string, S0.1 Action Envelope         |
| Produces       | `ActionEnvelope` requests with immutable `identity` and `request`; no execution mutations |
| Supersedes     | Rev.1 capability translator sketch                                                        |

## 1. Purpose and scope

The Capability Translator converts user intent, plan steps, and system context into typed AIOS action envelopes. It is the boundary that prevents the Cognitive Core from becoming a shell-command generator.

The translator answers one question:

```text
Given this goal and this context, which typed AIOS action(s) should be requested?
```

It does not answer:

```text
May this action run?
Did this action run?
Did verification pass?
```

Those are L4 Policy Kernel, L3 Capability Runtime, and L9 Verification/Evidence responsibilities.

Scope:

- Map natural-language goals and planner steps to known capability actions.
- Retrieve relevant capabilities from a large capability catalog.
- Bind action targets against adapter-declared schemas.
- Produce S0.1-compatible `ActionEnvelope` objects.
- Infer conservative risk hints, verification intents, sandbox hints, and dry-run mode.
- Detect ambiguity and missing information.
- Refuse unknown, untyped, or unsafe-by-construction translations.
- Record translation evidence without leaking secrets.

Out of scope:

- Final policy decisions.
- Human approval delivery.
- Adapter execution.
- Verification result evaluation.
- Secret retrieval or secret material exposure.
- Free-form shell generation.
- Implementation planning.

## 2. Position in the system

The Capability Translator sits inside L5 and produces envelopes consumed by L3.

```text
Human / Agent / Renderer
        |
        v
Intent Engine
        |
        v
Planner / Orchestrator
        |
        v
Capability Translator
        |
        v
ActionEnvelope.request
        |
        v
Capability Runtime -> Policy Kernel -> Adapter -> Verification -> Evidence
```

The translator is cognitive, not authoritative. It may propose. It may clarify. It may refuse to translate. It may not execute, approve, or bypass policy.

## 3. Core invariant

The translator must never output an executable command string as the primary action.

Rejected output:

```text
sudo systemctl restart nginx
```

Required output:

```json
{
  "action": "service.restart",
  "target": { "service": "nginx" },
  "reason": "Restart nginx after configuration update",
  "verification": [
    { "type": "service.active", "args": { "service": "nginx" } }
  ]
}
```

Shell commands may appear only as non-authoritative explanatory text in debugging evidence. They must not be passed to execution as the action primitive.

## 4. Terminology

| Term                 | Meaning                                                                                   |
| -------------------- | ----------------------------------------------------------------------------------------- |
| Capability           | A typed operation the system can request, such as `service.restart` or `package.install`. |
| Adapter              | L3 implementation provider for one or more capabilities, such as `systemd.local`.         |
| Capability catalog   | Indexed union of adapter manifests plus human-facing descriptions and examples.           |
| Translation          | Mapping from intent/plan/context to one or more action envelopes.                         |
| Binding              | Filling `target` fields according to the selected capability schema.                      |
| Clarification        | A structured question returned when translation would require guessing.                   |
| Refusal              | A structured rejection returned when no safe typed translation exists.                    |
| Action draft         | An `ActionEnvelope` with caller-owned fields populated and `execution` unset.             |

## 5. Inputs and outputs

### 5.1. Translation request

The translator accepts a structured request. Renderers and agents may originate it, but the Intent Engine or Planner should normalize it before translation.

```proto
syntax = "proto3";
package aios.cognition.v1alpha1;

import "google/protobuf/struct.proto";
import "aios/action/v1alpha1/action.proto";

message TranslateIntentRequest {
  string schema_version = 1;          // "aios.cognition.v1alpha1"
  string intent_id = 2;               // "intent_<ULID>"; required
  string plan_id = 3;                 // "plan_<ULID>"; optional for single-action intents
  string plan_step_id = 4;            // planner-owned stable step id; optional
  string correlation_id = 5;          // inherited from intent or workflow
  string subject = 6;                 // provisional L4 subject string
  string utterance = 7;               // user-facing goal or plan-step text
  ContextSnapshot context = 8;
  TranslationMode mode = 9;
  repeated TranslationConstraint constraints = 10;
  string preferred_catalog_version = 11;
}

message ContextSnapshot {
  string active_project_id = 1;
  string host_id = 2;
  string working_directory_object_id = 3;
  repeated string visible_resource_refs = 4;
  google.protobuf.Struct facts = 5;
}

enum TranslationMode {
  TRANSLATION_MODE_UNSPECIFIED = 0;
  SINGLE_ACTION = 1;      // exactly one action expected
  MULTI_ACTION = 2;       // translator may return ordered action drafts
  VALIDATION_ONLY = 3;    // validate an already proposed envelope
  EXPLAIN_ONLY = 4;       // explain candidate mapping; no envelope output
}

message TranslationConstraint {
  string key = 1;         // e.g. "dry_run", "environment", "preferred_adapter"
  string value = 2;
}
```

`utterance` is not trusted input. It is a semantic hint. The catalog, schemas, policy tags, and context snapshot constrain what can be produced.

### 5.2. Translation result

```proto
message TranslateIntentResponse {
  TranslationStatus status = 1;
  repeated ActionDraft action_drafts = 2;
  repeated ClarificationQuestion questions = 3;
  TranslationRefusal refusal = 4;
  TranslationEvidence evidence = 5;
}

enum TranslationStatus {
  TRANSLATION_STATUS_UNSPECIFIED = 0;
  READY = 1;                 // action_drafts are usable by L3 SubmitAction
  NEEDS_CLARIFICATION = 2;   // questions must be answered before translation
  REJECTED = 3;              // no valid typed translation exists
  PARTIAL = 4;               // some actions ready, some blocked by clarification
}

message ActionDraft {
  aios.action.v1alpha1.ActionEnvelope envelope = 1;
  string selected_capability_id = 2;
  string selected_adapter_family = 3;
  double confidence = 4;                 // 0.0..1.0; advisory, never policy
  repeated AlternativeCapability alternatives = 5;
  repeated string assumptions = 6;
  repeated string warnings = 7;
}

message AlternativeCapability {
  string capability_id = 1;
  string action = 2;
  double score = 3;
  string reason_not_selected = 4;
}

message ClarificationQuestion {
  string question_id = 1;
  string prompt = 2;
  repeated string allowed_values = 3;
  bool required = 4;
  string blocks_field = 5;                // e.g. "request.target.service"
}

message TranslationRefusal {
  string code = 1;
  string message = 2;
  repeated string evidence_refs = 3;
}

message TranslationEvidence {
  string translation_id = 1;              // "trn_<ULID>"
  string catalog_version = 2;
  repeated string retrieved_capability_ids = 3;
  repeated string selected_capability_ids = 4;
  repeated string model_ids = 5;
  repeated string prompt_hashes = 6;
  repeated string context_object_refs = 7;
}
```

The response is successful only when `status = READY` and every `ActionDraft.envelope.request` validates against S0.1 and the selected capability schema.

## 6. Capability catalog

The catalog is the translator's source of truth. It is built from adapter manifests and curated semantic metadata.

### 6.1. Capability manifest

Each capability has one canonical manifest.

```yaml
capability_id: service.restart.v1
action: service.restart
status: stable
version: 1
adapter_families:
  - systemd.local
  - openrc.local
target_schema:
  type: object
  required: [service]
  additionalProperties: false
  properties:
    service:
      type: string
      pattern: "^[a-zA-Z0-9_.@-]+$"
semantic:
  title: Restart a local service
  description: Restart an existing service managed by the host service runtime.
  aliases:
    - restart daemon
    - reload service by restart
    - bounce service
  positive_examples:
    - restart nginx
    - restart the docker service
  negative_examples:
    - install nginx
    - open port 443
risk_template:
  destructive: false
  privileged: true
  network_exposure: false
  secret_access: false
  recovery_path_affected: false
default_verification:
  - type: service.active
    args_from_target:
      service: service
default_sandbox_profile_id: host-service-control
policy_tags:
  - service-control
  - privileged
```

### 6.2. Required manifest fields

| Field                        | Required | Purpose                                                    |
| ---------------------------- | -------- | ---------------------------------------------------------- |
| `capability_id`              | yes      | Stable catalog identity.                                   |
| `action`                     | yes      | S0.1 `request.action` value.                               |
| `status`                     | yes      | `experimental`, `stable`, `deprecated`, or `retired`.      |
| `adapter_families`           | yes      | Adapter classes that can implement the action.             |
| `target_schema`              | yes      | JSON Schema used for binding validation.                   |
| `semantic.title`             | yes      | Human-readable title.                                      |
| `semantic.description`       | yes      | Retrieval and explanation text.                            |
| `semantic.positive_examples` | yes      | Retrieval anchors and evaluation examples.                 |
| `semantic.negative_examples` | yes      | Anti-match training data.                                  |
| `risk_template`              | yes      | Conservative caller risk defaults.                         |
| `default_verification`       | yes      | Verification intents to seed the envelope.                 |
| `default_sandbox_profile_id` | yes      | Sandbox hint if caller provides none.                      |
| `policy_tags`                | yes      | Policy-facing labels; not a policy decision.               |

Unknown manifest fields are rejected in stable catalogs. Experimental catalogs may allow `x_` extension fields.

### 6.3. Catalog versioning

Catalog versions are content-addressed:

```text
catalog_version = "cat_" + BLAKE3(canonical_manifest_set)[:32]
```

The translator must include the `catalog_version` in `TranslationEvidence`.

If a translation is retried with a different catalog version, the translator may reuse the previous result only if:

1. The selected `capability_id` still exists.
2. The `target_schema` is backward compatible.
3. The default risk and verification templates did not become stricter.

Otherwise, it must retranslate.

## 7. Translation pipeline

The translator uses a deterministic outer pipeline with optional model calls inside bounded steps.

```text
Receive request
  -> normalize utterance and context
  -> retrieve candidate capabilities
  -> rank candidates
  -> bind target fields
  -> validate target schema
  -> infer risk, verification, sandbox, dry_run
  -> construct ActionEnvelope identity/request
  -> validate S0.1 envelope invariants
  -> return READY / NEEDS_CLARIFICATION / REJECTED
```

### 7.1. Pipeline stages

| Stage            | Input                         | Output                         | Failure mode                              |
| ---------------- | ----------------------------- | ------------------------------ | ----------------------------------------- |
| Normalize        | utterance, context            | normalized translation query   | `InvalidTranslationRequest`               |
| Retrieve         | normalized query, catalog     | candidate capabilities         | `NoCapabilityCandidates`                  |
| Rank             | candidates, context           | ordered candidates             | `AmbiguousCapability`                     |
| Bind             | best candidate, context       | target object                  | `MissingTargetField`                      |
| Validate         | target, schema, S0.1          | valid request fields           | `TargetSchemaInvalid`                     |
| Risk             | manifest, target, context     | conservative risk declaration  | `RiskInferenceFailed`                     |
| Verification     | manifest, target, utterance   | verification intents           | `VerificationIntentUnavailable`           |
| Envelope build   | request fields, identifiers   | action draft envelope          | `EnvelopeBuildFailed`                     |
| Evidence project | all previous stages           | translation evidence           | fail-closed; no READY without evidence    |

### 7.2. Determinism rule

Model output is never accepted directly. It must be projected into structured fields and validated against:

1. Capability catalog membership.
2. Target JSON Schema.
3. S0.1 action envelope schema.
4. Translator invariants in this document.

If validation fails, the translator must not "fix and execute" silently. It either retries translation internally with the validation error as context or returns `NEEDS_CLARIFICATION` / `REJECTED`.

## 8. Retrieval and ranking

The translator must support thousands of capabilities without relying on one giant prompt.

### 8.1. Index fields

Each capability is indexed by:

- `action`
- `capability_id`
- title and description
- aliases
- positive examples
- negative examples
- target schema field names
- policy tags
- adapter families
- layer ownership
- deprecation status

### 8.2. Retrieval strategy

Retrieval uses a hybrid strategy:

| Signal             | Purpose                                             |
| ------------------ | --------------------------------------------------- |
| Exact action match | User or planner already named `service.restart`.    |
| Lexical match      | Fast match on service/package/network terms.        |
| Embedding match    | Semantic match for phrasing variation.              |
| Schema fit         | Whether required target fields can be filled.       |
| Context fit        | Whether referenced resources exist in context.      |
| Negative examples  | Penalize similar but wrong actions.                 |
| Status penalty     | Penalize deprecated and experimental capabilities.  |

Vector similarity alone must never be the final authority.

### 8.3. Ranking thresholds

Default thresholds:

| Condition                         | Result                         |
| --------------------------------- | ------------------------------ |
| top score >= 0.86 and margin >= 0.10 | select top candidate           |
| top score >= 0.70 but margin < 0.10  | return `NEEDS_CLARIFICATION`   |
| top score < 0.70                    | return `REJECTED`              |
| required target fields missing      | return `NEEDS_CLARIFICATION`   |
| selected capability deprecated      | select only if no stable match and warning emitted |

Implementations may tune numeric thresholds, but they must preserve the behaviors: high-confidence selection, ambiguity clarification, low-confidence refusal.

## 9. Target binding

Target binding fills `request.target`.

### 9.1. Binding sources

Allowed sources:

- Explicit user text.
- Planner step structured fields.
- Current UI selection.
- Active project context.
- System Knowledge Graph facts.
- Previous approved intent context.
- Adapter manifest defaults.

Forbidden sources:

- Raw secrets.
- Hidden prompt text.
- Unverified model guesses.
- Shell command fragments as target payloads unless the selected action explicitly models command execution. Rev.2 does not define such an action.

### 9.2. Missing fields

If a required target field cannot be filled, the translator returns `NEEDS_CLARIFICATION`.

Example:

```text
User: "restart the service"
Candidates: service.restart
Missing: target.service
Question: "Which service should be restarted?"
```

The translator must not guess `nginx` because it was recently active unless the active context explicitly marks nginx as the selected service.

### 9.3. Schema validation

The selected capability's `target_schema` is mandatory. A translation cannot be `READY` until:

1. Required fields are present.
2. No forbidden additional fields are present.
3. Types validate.
4. Patterns validate.
5. Cross-field constraints validate where declared.

## 10. Risk, verification, sandbox, and dry-run

### 10.1. Risk declaration

Risk fields in S0.1 are caller claims, not authoritative policy. The translator must make conservative claims.

Rules:

- It may overstate risk.
- It must not knowingly understate risk.
- It must merge manifest risk, target-specific risk, and context-specific risk.
- If risk cannot be determined, set the relevant risk flag to `true` and add a warning.

Examples:

| Action                   | Risk inference                                      |
| ------------------------ | --------------------------------------------------- |
| `service.restart`        | `privileged=true`                                   |
| `package.install`        | `privileged=true`                                   |
| `network.firewall.allow` | `privileged=true`, `network_exposure=true`          |
| `secret.rotate`          | `privileged=true`, `secret_access=true`             |
| `aiosfs.pointer.rollback`| `destructive=true` if pointer affects live data     |

### 10.2. Verification intent generation

Verification intents are composed from:

1. Capability manifest defaults.
2. User-stated success criteria.
3. Planner step expected outcome.
4. Context-specific checks.

The translator should include at least one verification intent for every state-changing action unless the manifest marks verification as unavailable.

Examples:

| Action            | Default verification                              |
| ----------------- | ------------------------------------------------- |
| `service.restart` | `service.active`                                  |
| `package.install` | `package.installed`                               |
| `network.expose`  | `port.open` plus optional `http.ok`               |
| `repo.clone`      | `repo.exists` and `repo.clean` when applicable    |

If no verification grammar exists yet, the translator uses provisional `{ type, args }` names and marks them as dependent on L9 S2.4.

### 10.3. Sandbox profile

The translator may suggest `sandbox_profile_id`, but L3/L4 choose the applied profile.

Selection order:

1. User or planner constraint, if present.
2. Capability manifest default.
3. Runtime default.

The translator must not choose a less restrictive profile to improve convenience. Policy may override toward stricter profiles.

### 10.4. Dry-run mode

Default is `LIVE` only when the user intent is clearly operational.

The translator should select:

| User intent pattern                       | `dry_run` |
| ----------------------------------------- | --------- |
| "do it", "install", "restart", "apply"    | `LIVE`    |
| "can you", "would this work", "check"     | `VALIDATE` or `SIMULATE` |
| explicit "simulate" or "dry run"          | `SIMULATE` |
| high-risk ambiguous request               | `SIMULATE` or clarification |

Policy can still require approval for `SIMULATE` if the simulated path touches sensitive metadata.

## 11. Identity, idempotency, and causality

The translator constructs the caller-owned S0.1 `identity` fields.

### 11.1. `action_id`

`action_id` is a new `act_<ULID>` per envelope draft.

### 11.2. `idempotency_key`

The idempotency key is stable for one logical plan step retry.

Recommended canonical input:

```json
{
  "intent_id": "intent_...",
  "plan_id": "plan_...",
  "plan_step_id": "step_install_docker",
  "action": "package.install",
  "target": { "package": "docker" },
  "dry_run": "LIVE"
}
```

Canonical form:

```text
idempotency_key = "idem_" + BLAKE3(JCS(canonical_input))[:32]
```

If the planner intentionally changes the step meaning, it must change `plan_step_id` or `plan_id`, producing a new idempotency key.

### 11.3. Causality

For multi-action plans:

- First action: `parent_action_id` unset.
- Later actions: `parent_action_id` references the action that directly caused this action when there is a strict dependency.
- Independent parallel actions share `plan_id` and `correlation_id` but do not set each other as parents.

Saga composition and multi-parent causality are deferred.

## 12. Ambiguity handling

The translator must prefer clarification over confident guessing.

Clarification is required when:

- Two or more capabilities are plausible and the ranking margin is below threshold.
- A required target field is missing.
- A target value is ambiguous.
- The user asks for a broad outcome that requires planning but translation mode is `SINGLE_ACTION`.
- The action would affect network exposure, secrets, recovery path, or destructive data movement and the user's target is underspecified.

Example:

```text
User: "make the app public"
Possible translations:
- network.expose
- service.start
- firewall.allow
- reverse_proxy.route.add

Result: NEEDS_CLARIFICATION
Question: "Which app and which public hostname or port should be exposed?"
```

## 13. Refusal rules

The translator returns `REJECTED` when it cannot produce a typed action safely.

Canonical refusal codes:

| Code                        | Meaning                                                     |
| --------------------------- | ----------------------------------------------------------- |
| `NoMatchingCapability`      | No catalog capability maps to the request.                  |
| `UnknownActionRequested`    | User named an action that does not exist in the catalog.    |
| `UntypedShellRequested`     | Request requires free-form shell execution.                 |
| `SecretExfiltrationRequest` | Request asks to reveal or export secret material.           |
| `PolicyBypassRequested`     | Request asks to skip approval, logging, sandbox, or policy. |
| `RecoveryPathUnsafe`        | Request would alter recovery path outside defined actions.  |
| `TargetSchemaImpossible`    | Required schema cannot be satisfied.                        |

Refusal is not a policy decision. It is a translator safety outcome: no valid typed action draft exists.

## 14. API surface

The translator exposes a small service API. Names are conceptual; transport is gRPC unless a local in-process implementation is used.

```proto
service CapabilityTranslator {
  rpc TranslateIntent(TranslateIntentRequest) returns (TranslateIntentResponse);
  rpc ValidateDraft(ValidateDraftRequest) returns (ValidateDraftResponse);
  rpc ExplainTranslation(ExplainTranslationRequest) returns (ExplainTranslationResponse);
  rpc ListMatchingCapabilities(ListMatchingCapabilitiesRequest) returns (ListMatchingCapabilitiesResponse);
}

message ValidateDraftRequest {
  aios.action.v1alpha1.ActionEnvelope envelope = 1;
  string catalog_version = 2;
}

message ValidateDraftResponse {
  bool valid = 1;
  repeated string errors = 2;
  repeated string warnings = 3;
}
```

`TranslateIntent` is the normal path. `ValidateDraft` supports renderer inspection and tests. `ExplainTranslation` is for user-facing "why this action" views. `ListMatchingCapabilities` supports debugging and catalog QA.

## 15. Evidence and privacy

Every translation attempt produces a translation evidence record.

Minimum fields:

- `translation_id`
- timestamp
- subject
- intent_id
- plan_id
- catalog_version
- retrieved capability IDs
- selected capability IDs
- final status
- confidence
- refusal code or clarification question IDs
- model IDs used
- prompt hashes, not raw prompts by default
- context object references, not full context dumps by default

Privacy rules:

- Raw secrets must never appear in translation evidence.
- If user text contains a secret-like value, evidence stores a redacted projection.
- Prompt bodies are stored only when policy explicitly enables debugging capture.
- Stored prompts must pass the same redaction pipeline as user text.
- Translation evidence must link to resulting `action_id` values when `status = READY`.

## 16. Model-use discipline

The translator may use an LLM, but the LLM is not the authority.

Allowed model tasks:

- Parse user phrasing.
- Suggest candidate actions from retrieved catalog snippets.
- Extract target field candidates.
- Generate user-facing clarification questions.
- Explain why a capability was selected.

Forbidden model tasks:

- Invent actions not in the catalog.
- Invent adapter capabilities.
- Override target schema validation.
- Decide policy.
- Read or transform raw secrets.
- Produce shell commands for execution.

The direct path should bypass the LLM for exact, low-ambiguity inputs such as:

```text
restart nginx
install docker
show service status for postgresql
```

Latency tiering is specified in S1.2.

## 17. Examples

### 17.1. Restart nginx

Input:

```text
restart nginx
```

Output draft request:

```json
{
  "action": "service.restart",
  "target": { "service": "nginx" },
  "subject": "human:lucky",
  "reason": "Restart nginx as requested by the user",
  "environment": "LOCAL",
  "risk": {
    "destructive": false,
    "privileged": true,
    "network_exposure": false,
    "secret_access": false,
    "recovery_path_affected": false
  },
  "verification": [
    { "type": "service.active", "args": { "service": "nginx" } }
  ],
  "sandbox_profile_id": "host-service-control",
  "dry_run": "LIVE"
}
```

### 17.2. Install Docker

Input:

```text
prepare docker on this machine
```

Possible action drafts:

```text
package.install { package: "docker" }
service.enable  { service: "docker" }
service.start   { service: "docker" }
```

If translation mode is `SINGLE_ACTION`, return `NEEDS_CLARIFICATION` or the first action only depending on planner context. If mode is `MULTI_ACTION`, return an ordered set with causality links.

### 17.3. Use an SSH key without revealing it

Input:

```text
clone the private repo using my github ssh key
```

Allowed translation:

```json
{
  "action": "secret.use.ssh_key_for_git",
  "target": {
    "repo": "git@github.com:org/repo.git",
    "destination": "aiosfs://projects/org/repo",
    "key_ref": "vault://user/github/default"
  },
  "risk": {
    "secret_access": true,
    "privileged": false
  }
}
```

Rejected translation:

```text
cat ~/.ssh/id_rsa
```

### 17.4. Ambiguous public exposure

Input:

```text
put this app online
```

Result:

```json
{
  "status": "NEEDS_CLARIFICATION",
  "questions": [
    {
      "question_id": "q_target_app",
      "prompt": "Which app should be exposed?",
      "required": true,
      "blocks_field": "request.target.app"
    },
    {
      "question_id": "q_public_endpoint",
      "prompt": "Which hostname or port should be used?",
      "required": true,
      "blocks_field": "request.target.endpoint"
    }
  ]
}
```

## 18. Cross-layer dependencies

| Layer / spec | Dependency                                                                 |
| ------------ | -------------------------------------------------------------------------- |
| L0           | Uses status taxonomy and evidence-grade discipline.                        |
| L3           | Consumes adapter manifests and submits envelopes to Capability Runtime.     |
| L4           | Uses provisional `subject`; final identity and policy are L4 concerns.      |
| L5           | Owned by Cognitive Core; integrates with Intent Engine and Planner.         |
| L9           | Emits translation evidence and verification intents.                        |
| S0.1         | Must produce valid `ActionEnvelope` identity/request sections.              |
| S1.2         | Latency tiering decides direct path vs cognitive path.                      |
| S1.3         | AIOS-FS object actions require object-model-specific target schemas.        |

## 19. Invariants

1. The translator never executes actions.
2. The translator never approves actions.
3. The translator never reads raw secrets.
4. The translator never emits unknown actions.
5. The translator never treats model output as authoritative.
6. Every `READY` result includes at least one valid action draft.
7. Every action draft validates against the selected capability schema.
8. Every action draft validates against S0.1 envelope request rules.
9. Ambiguity returns clarification, not guessed execution.
10. Risk hints are conservative.
11. Translation evidence is emitted for every attempt.
12. Translation evidence is redacted by default.

## 20. Acceptance criteria

This sub-spec is satisfied when an implementation can demonstrate:

- A capability catalog with at least service, package, network, repo, secret, and AIOS-FS actions.
- Exact translation without an LLM for common commands such as `restart nginx`.
- RAG-assisted translation across at least 1000 capabilities.
- Schema-valid `ActionEnvelope` drafts for selected capabilities.
- Clarification on missing target fields.
- Refusal on free-form shell execution.
- Conservative risk inference.
- Default verification intent generation.
- Stable idempotency keys for logical retries.
- Translation evidence with catalog version, selected capability ID, and redacted prompt/context metadata.

## 21. Open deferrals

- Full canonical subject identity belongs to L4.
- Complete verification grammar belongs to L9 S2.4.
- Latency budgets and direct-vs-LLM routing belong to S1.2.
- AIOS-FS object target schemas belong to S1.3.
- Approval UI and approval receipt mechanics belong to L4.
- Resource budget hints remain deferred from S0.1.

## 22. See also

- [S0.1 Action Envelope + Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [L5 Cognitive Core overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
- [Rev.1 §12.5 and §13](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
