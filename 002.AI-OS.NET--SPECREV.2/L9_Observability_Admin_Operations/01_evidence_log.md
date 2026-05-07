# Evidence Log Architecture (Rev.2)

| Field     | Value                                  |
| --------- | -------------------------------------- |
| Status    | `CONTRACT` draft                       |
| Phase tag | S3.1                                   |
| Layer     | L9 Observability, Admin, Operations    |
| Consumes  | action envelopes, policy decisions, verification results |
| Produces  | append-only evidence receipts and indexes |

## 1. Purpose

The Evidence Log is AIOS's operational memory of what happened. It records requests, decisions, denials, approvals, execution facts, verification results, failures, recovery events, and model routing decisions.

## 2. Core invariant

Evidence is append-only. AI agents cannot edit or delete evidence.

Corrections are new evidence records that reference older records.

## 3. Record shape

```json
{
  "receipt_id": "evr_...",
  "timestamp": "...",
  "record_type": "action.received",
  "subject": "human:lucky",
  "action_id": "act_...",
  "policy_decision_id": "poldec_...",
  "verification_ref": null,
  "payload_hash": "blake3:...",
  "payload_ref": "segment://...",
  "redaction_profile": "default",
  "previous_receipt_hash": "blake3:..."
}
```

## 4. Architecture

```text
WAL
  -> sealed segments
  -> hash chain
  -> indexes
  -> cold archive
```

The WAL is optimized for durable append. Sealed segments are immutable. Indexes are rebuildable.

## 5. Record types

| Type                    | Meaning                         |
| ----------------------- | ------------------------------- |
| `action.received`       | envelope accepted               |
| `translation.created`   | S1.1 translator produced result |
| `routing.decision`      | S1.2 selected cognition tier    |
| `policy.decision`       | allow/approval/deny             |
| `approval.requested`    | human approval requested        |
| `approval.granted`      | approval granted                |
| `approval.denied`       | approval denied                 |
| `execution.started`     | adapter started                 |
| `execution.completed`   | adapter completed               |
| `verification.result`   | verification result             |
| `rollback.completed`    | rollback completed              |
| `recovery.event`        | recovery mode or repair action  |
| `model.call`            | model routing and usage metadata |

## 6. Redaction

Evidence stores references and hashes by default. Sensitive payloads are redacted.

Never store:

- raw secret values
- private keys
- tokens
- passwords
- full prompt bodies containing secrets

Debug capture is a policy-controlled mode, not default behavior.

## 7. Indexes

Indexes are rebuildable and may include:

- action id
- intent id
- plan id
- correlation id
- subject
- policy decision id
- object id
- service name
- timestamp
- status
- record type

Index corruption must not corrupt evidence truth. Reindex from sealed segments.

## 8. Compaction

Compaction may create summaries and tier old payloads to cold storage, but it must not remove the receipt chain.

Allowed:

- build summary records
- move payloads to archive
- rebuild indexes

Forbidden:

- delete receipt identity
- rewrite past decisions
- remove denials or failures
- break hash chain

## 9. Acceptance criteria

- Every action has evidence from receipt to terminal phase.
- Denials and failures are logged.
- Evidence receipts form a verifiable chain.
- Indexes can be rebuilt.
- Secret redaction is default.
- Recovery mode can read evidence without Cognitive Core.

