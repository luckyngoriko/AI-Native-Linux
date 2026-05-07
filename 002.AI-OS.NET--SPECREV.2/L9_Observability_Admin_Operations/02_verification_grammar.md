# Verification Grammar (Rev.2)

| Field     | Value                                  |
| --------- | -------------------------------------- |
| Status    | `CONTRACT` draft                       |
| Phase tag | S2.4                                   |
| Layer     | L9 Observability, Admin, Operations    |
| Consumes  | S0.1 verification intents              |
| Produces  | typed verification results             |

## 1. Purpose

Verification proves that an action produced its intended result. AIOS does not treat successful execution as success unless verification passes or is explicitly skipped by policy.

## 2. Verification intent

S0.1 carries:

```json
{
  "type": "service.active",
  "args": { "service": "nginx" }
}
```

This spec defines the canonical vocabulary and composition rules.

## 3. Primitive vocabulary

| Type                | Required args                 | Success condition                         |
| ------------------- | ----------------------------- | ----------------------------------------- |
| `service.active`    | `service`                     | service manager reports active             |
| `service.inactive`  | `service`                     | service manager reports inactive           |
| `package.installed` | `package`                     | package query returns installed            |
| `port.open`         | `host`, `port`, `protocol`    | connection succeeds                        |
| `port.closed`       | `host`, `port`, `protocol`    | connection fails as expected               |
| `http.ok`           | `url`                         | HTTP status is 2xx or declared expected    |
| `file.exists`       | `object_or_path`              | object/path exists                         |
| `file.hash`         | `object_or_path`, `hash`      | content hash matches                       |
| `repo.exists`       | `path_or_object`              | repository metadata exists                 |
| `aiosfs.pointer`    | `object_id`, `pointer`, `version_id` | pointer targets expected version     |
| `policy.decision`   | `policy_decision_id`, `decision` | decision matches expected              |
| `evidence.exists`   | `receipt_id`                  | evidence receipt is present and valid      |

## 4. Composition

Verification supports:

```yaml
all:
  - type: service.active
    args: { service: nginx }
  - type: http.ok
    args: { url: http://localhost/ }
```

Combinators:

- `all`
- `any`
- `not`
- `eventually`

`eventually` requires timeout and interval.

## 5. Result shape

```json
{
  "intent": { "type": "service.active", "args": { "service": "nginx" } },
  "status": "passed | failed | timeout | skipped",
  "reason": "ActiveStateObserved",
  "observed": { "active_state": "active" },
  "verified_at": "..."
}
```

Observed data is redacted before evidence storage.

## 6. Property-based verification

Some actions require invariant checks rather than fixed expected values.

Examples:

- evidence log remains append-only after compaction
- AIOS-FS pointer history remains acyclic
- policy default deny still denies unknown action

Property checks must be deterministic and repeatable.

## 7. Acceptance criteria

- Every state-changing capability has a default verification intent or explicit reason why not.
- Verification results map back to S0.1 `request.verification`.
- Timeout is a first-class result.
- Skipped verification is evidence and requires policy allowance.
- Verification cannot mutate the system except through explicitly declared read probes.

