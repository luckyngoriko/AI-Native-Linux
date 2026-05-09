# Approval Mechanics (Rev.2)

| Field          | Value                                                                                                                                                                    |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Status         | `REAL` (E1 — file exists; structural contract complete; written 2026-05-09)                                                                                              |
| Phase tag      | S5.3                                                                                                                                                                     |
| Layer          | L4 Policy, Identity, Vault                                                                                                                                               |
| Schema package | `aios.approval.v1alpha1`                                                                                                                                                 |
| Consumes       | S0.1 action envelope + canonical hash, S2.3 policy decision + `request_approval` outcome, S5.1 identity model (Subject), S7.1 surface composition, S7.2 shared UI schema |
| Produces       | typed `ApprovalRequest`, `ApprovalBinding`, FSM transitions, channel selection, evidence records APPROVAL\_\*                                                            |

## §1 Purpose

The Policy Kernel ([L4.1 §15](./01_policy_kernel.md)) decides — for every typed action — one of `ALLOW`, `REQUIRE_APPROVAL`, or `DENY`. When the decision is `REQUIRE_APPROVAL`, control flow leaves the kernel and enters this sub-spec. Approval Mechanics is the contract that defines:

1. How an approval request is constructed from a policy decision plus the action canonical hash.
2. How the request is delivered to a human (and only a human) on a trust-bearing surface.
3. How the human's grant or denial is bound, single-use, time-bounded, and evidence-linked.
4. How the binding voids if the underlying action mutates between grant and execute time.
5. How the binding terminates: CONSUMED, EXPIRED, REVOKED, or DENIED.

This sub-spec is the **only** place in AIOS where a typed action can move from `policy_pending` to `executing` for actions whose policy outcome was `REQUIRE_APPROVAL`. There is no other path. The Capability Runtime ([L3](../L3_AIOS_SGR_Service_Graph_Runtime/00_overview.md)) is forbidden from advancing an action whose policy outcome was `REQUIRE_APPROVAL` without consuming a valid `ApprovalBinding` produced here.

This sub-spec **does not** define:

- The Policy Kernel decision pipeline (that is L4.1).
- The Vault Broker capability operations (that is S5.2 / `02_vault_broker.md`).
- Emergency Override mechanics for hard-denied actions (that is S5.4 / `05_emergency_override.md`).
- The visual treatment of the approval surface (that is L7.3 Visual Language).
- The wire format of the renderer's UI tree (that is L7.2 Shared UI Schema).

This sub-spec **does** define the structural contract that all of those depend on: the binding record, the FSM, the channel-selection rules, the TTL discipline, and the evidence wire-up.

## §2 Scope

In scope:

- The `ApprovalRequest` and `ApprovalBinding` records (closed schemas).
- The `ApprovalRequestState` FSM (closed enum) and its legal transitions.
- The `ApprovalChannel` taxonomy (closed enum) and the deterministic channel-selection algorithm.
- The `ApprovalStrength` taxonomy (closed enum) and how each strength tier wires into authentication and dual-control.
- The `ApprovalBindingScope` taxonomy (closed enum) and what each scope spends.
- The `ApprovalDenialReason` taxonomy (closed enum).
- The `ApprovalTtlClass` taxonomy (closed enum) and the recommended defaults table.
- The action-revision invariant: any change to the canonical action hash voids the binding.
- The trust-bearing surface contract: who can author the prompt, in which composition zone, with which `is_ai_origin` value.
- The closed list of evidence record types this sub-spec emits and their retention class.
- Revocation semantics for `GRANTED` bindings before consumption.
- Dual-control semantics (two distinct human subjects).
- Anti-replay: `EXACT_ACTION` bindings are single-use and `CONSUMED` is terminal.
- The boundary against hard-denied actions (no approval rescue; redirect to S5.4).

Out of scope:

- Emergency override flows (S5.4).
- Vault capability issuance triggered by a granted approval (S5.2).
- Policy bundle authoring (L4.1 §11–§12).
- Renderer-specific implementations of the prompt (L7.4 KDE, L7.5 Web, L7.6 CLI).
- Voice and mobile delivery wire formats (channels reserved, deferred).

## §3 Vocabulary

This section declares the closed enums on which the rest of the sub-spec is built. Each enum is contract-grade: adding a value is a versioned spec change; removing a value is a recovery-mode invariant-bundle update.

### §3.1 `ApprovalRequestState`

The finite-state machine of an approval request. Closed enum, eight states.

| Value               | Semantics                                                                                                                                 |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `DRAFT`             | Created by Policy Kernel; surface not yet delivered to operator.                                                                          |
| `AWAITING_OPERATOR` | Surface delivered through the selected `ApprovalChannel`; operator has not responded; TTL clock is running.                               |
| `GRANTED`           | Operator approved; `ApprovalBinding` is active and the bound action may proceed under the binding's scope and TTL.                        |
| `DENIED`            | Operator rejected, or the request transitioned to a denied state via TTL/scope/revision (see `ApprovalDenialReason`).                     |
| `EXPIRED`           | TTL elapsed in `AWAITING_OPERATOR` before the operator responded; equivalent to `DENIED` with reason `TTL_EXPIRED`.                       |
| `REVOKED`           | A previously-active `GRANTED` binding was revoked before `CONSUMED` (operator self-revoke or higher-priority subject).                    |
| `CONSUMED`          | The binding was spent on the exact action it was bound to. Terminal success state.                                                        |
| `FAILED_DELIVERY`   | The Approval Mechanics service could not deliver the prompt to any allowed channel; equivalent to `DENIED` with reason `DELIVERY_FAILED`. |

Terminal states: `DENIED`, `EXPIRED`, `REVOKED`, `CONSUMED`, `FAILED_DELIVERY`. Once a request is in a terminal state, the record is sealed in evidence and the binding (if any) is voided.

### §3.2 `ApprovalChannel`

The closed set of channels through which an approval prompt may be delivered. Channels are physical surfaces; the Approval Mechanics service selects exactly one channel per request via §7.

| Value                 | Semantics                                                                                                       |
| --------------------- | --------------------------------------------------------------------------------------------------------------- |
| `KDE_NATIVE_PROMPT`   | KDE Plasma trust-bearing surface in the CHROME composition zone; default for a local human user at the console. |
| `WEB_LOOPBACK_PROMPT` | Web renderer at `127.0.0.1` only; default for a non-KDE local human user.                                       |
| `WEB_LAN_PROMPT`      | Web renderer over LAN; requires explicit `WEB_EXPOSURE_GRANTED` evidence and a policy clearance.                |
| `CLI_TTY_PROMPT`      | CLI renderer attached to a controlling TTY; used when no graphical session is available.                        |
| `MOBILE_PUSH`         | Push notification to a bound mobile device. Channel reserved; wire format deferred to a future revision.        |
| `VOICE_CHALLENGE`     | Voice renderer; a spoken challenge with a verbal grant phrase. Channel reserved; wire format deferred.          |
| `RECOVERY_CONSOLE`    | Recovery-mode TTY; usable only when the operator's session class is `RECOVERY` (S5.1 §7).                       |

Channel constitutional rules:

- `RECOVERY_CONSOLE` cannot be selected for normal-mode actions.
- `WEB_LAN_PROMPT` cannot be selected without an active `WEB_EXPOSURE_GRANTED` evidence record (INV-006).
- `MOBILE_PUSH` and `VOICE_CHALLENGE` are channel-reserved in Rev.2: a channel-selection algorithm that would otherwise pick them returns `FAILED_DELIVERY` until the wire format is defined.

### §3.3 `ApprovalStrength`

The closed strength tiers that a policy decision may attach to an approval requirement.

| Value       | Semantics                                                                                                                 |
| ----------- | ------------------------------------------------------------------------------------------------------------------------- |
| `WEAK`      | Single click or tap by a single subject. Suitable only for low-risk, reversible actions.                                  |
| `STRONG`    | Explicit phrase or step-up reauthentication required. The subject's session class must be at `STRONG` or above (S5.1 §8). |
| `DUAL`      | Two distinct human subjects must independently grant. Both signatures present in the binding.                             |
| `BIOMETRIC` | `STRONG` plus a biometric step (TouchID-class or hardware-attested). Wire format deferred; reserved value.                |

`BIOMETRIC` is reserved in Rev.2 (no implementation contract); a policy decision that demands `BIOMETRIC` falls back to `STRONG` with the additional constraint that the subject's session must have the `BIOMETRIC_REQUIRED` risk flag — until the biometric channel is contracted in a future revision.

### §3.4 `ApprovalDenialReason`

Closed reasons for denial, expiry, or void. Every record in `DENIED`, `EXPIRED`, `REVOKED`, or `FAILED_DELIVERY` carries exactly one of these.

| Value                 | Semantics                                                                                                     |
| --------------------- | ------------------------------------------------------------------------------------------------------------- |
| `OPERATOR_REJECTED`   | The operator explicitly denied the request through a trust-bearing surface.                                   |
| `TTL_EXPIRED`         | The TTL elapsed in `AWAITING_OPERATOR` before any response.                                                   |
| `ACTION_REVISED`      | The bound action's canonical hash changed between grant and execute; the binding voided automatically.        |
| `DELIVERY_FAILED`     | No channel could deliver the prompt; `FAILED_DELIVERY` terminal state.                                        |
| `SCOPE_DRIFT`         | The grant scope (subject × action_kind × resource_class) no longer matches the request scope at consume time. |
| `SUPERSEDED`          | Replaced by a higher-priority request before the operator could grant.                                        |
| `REVOKED_BY_OPERATOR` | The granting subject (or a higher-priority subject) revoked an active `GRANTED` binding before consumption.   |

### §3.5 `ApprovalBindingScope`

The closed set of scopes that determine what a granted binding may be spent on.

| Value            | Semantics                                                                                                         |
| ---------------- | ----------------------------------------------------------------------------------------------------------------- |
| `EXACT_ACTION`   | Bound to one `ActionRequestId` and one `bound_action_canonical_hash`; spent on first `ExecuteAction`. Single-use. |
| `ACTION_FAMILY`  | Bound to the tuple `(subject × action_kind × resource_class)` for a short TTL; multi-spend allowed within TTL.    |
| `SESSION_SCOPED` | Bound to a single Surface session id. Reserved in Rev.2; wire format deferred.                                    |

`ACTION_FAMILY` is the only scope that allows multi-spend, and the policy decision must explicitly request it through `Constraints.approval_scope = "action_family"` plus an explicit short TTL (§8). The default scope for any policy decision that does not specify is `EXACT_ACTION`.

### §3.6 `ApprovalTtlClass`

Closed TTL tiers. Each tier is a hard upper bound on the validity window from grant time. **There is no `TTL_INFINITE` tier; an infinite TTL is constitutionally forbidden.**

| Value          | Hard upper bound | Typical use                                                                                            |
| -------------- | ---------------- | ------------------------------------------------------------------------------------------------------ |
| `TTL_INSTANT`  | ≤ 60 s           | Time-critical, single-shot actions (e.g. operator-approved one-off command).                           |
| `TTL_SHORT`    | ≤ 5 min          | Default for most interactive grants; aligned with INV-009 default for the `INTERACTIVE` session class. |
| `TTL_MEDIUM`   | ≤ 30 min         | Multi-step workflows where a single human approver authorises a short batch.                           |
| `TTL_LONG`     | ≤ 4 h            | Long-running workflows under explicit policy clearance; requires `STRONG` strength.                    |
| `TTL_RECOVERY` | ≤ 30 min         | Recovery-mode operations only; cannot be used for normal-mode requests.                                |

Constitutional: every approval has a non-zero finite TTL. The Approval Mechanics service rejects an `ApprovalRequirement` with `ttl_seconds = 0` or `ttl_seconds > tier_upper_bound` at request creation; `DRAFT → DENIED(reason = TTL_EXPIRED)` is the canonical response, and the corresponding evidence is emitted before the prompt is even constructed.

## §4 ApprovalRequest record

The `ApprovalRequest` is what the Policy Kernel hands to Approval Mechanics. It is the short-lived workflow object whose lifecycle is the §6 FSM.

```proto
syntax = "proto3";
package aios.approval.v1alpha1;

import "google/protobuf/timestamp.proto";

message ApprovalRequest {
  // Identity --------------------------------------------------------------
  string approval_request_id = 1;        // "apprq_" + 26-char ULID base32 (S0.1 §3.2)
  string action_id = 2;                  // S0.1 ActionEnvelope.identity.action_id
  string action_request_id = 3;          // L3 Capability Runtime ActionRequestId
  string policy_decision_id = 4;         // L4.1 PolicyDecision.policy_decision_id
  string request_hash = 5;               // S0.1 §8.5 canonical request_hash
  string bundle_version = 6;             // L4.1 active bundle at request creation

  // Subject and scope ------------------------------------------------------
  string proposing_subject_id = 7;       // canonical subject id of the action's emitter
  string approver_subject_filter = 8;    // closed set: HUMAN_USER required; SubjectKind filter
  ApprovalStrength strength = 9;
  ApprovalBindingScope scope = 10;
  ApprovalTtlClass ttl_class = 11;
  uint32 ttl_seconds = 12;               // bounded by ApprovalTtlClass upper bound
  bool require_co_signer = 13;           // true iff strength = DUAL

  // Delivery ---------------------------------------------------------------
  ApprovalChannel selected_channel = 14;
  string surface_node_id = 15;           // L7.2 NodeKind = APPROVAL_PROMPT root

  // FSM and timing ---------------------------------------------------------
  ApprovalRequestState state = 16;
  google.protobuf.Timestamp created_at = 17;
  google.protobuf.Timestamp delivered_at = 18;
  google.protobuf.Timestamp expires_at = 19;

  // Evidence linkage -------------------------------------------------------
  string evidence_chain_root = 20;       // hash of prior evidence record at request creation

  // Reason on terminal -----------------------------------------------------
  ApprovalDenialReason denial_reason = 21;   // populated only in terminal denial states
  string denial_message = 22;                // English plain-text; never contains secrets
}
```

Identity rules:

- `approval_request_id` is `"apprq_" + ULID + 26-char base32` (per S0.1 §3.2 prefix-namespace registry); the ULID's time component MUST be the millisecond `created_at` clock the kernel observed when emitting the `request_approval` outcome. This binds the request id to a monotonic time anchor used by the FSM and TTL audit. The `apprq_` prefix is distinct from the `appb_` binding prefix (§5) and from the legacy `appr_` approval-receipt prefix in S0.1; the three namespaces never collide.
- `request_hash` is reproduced verbatim from S0.1 §8.5 (`hex_lower(BLAKE3(JCS(action)))[:32]`). The hash is the binding spine for the §11 anti-replay and the §13 action-revision invariant.
- `bundle_version` is the policy bundle that produced the underlying decision (L4.1 §12). A bundle flip after request creation does not change the bundle the request was authorised against; the request finishes on the version it started with, consistent with L4.1 §12.4.

State rules:

- A request is created in `DRAFT`. The Approval Mechanics service is the only authority that may transition it.
- `delivered_at` is unset in `DRAFT`; populated when the surface is acknowledged received by the chosen renderer.
- `expires_at` is the deadline of the `AWAITING_OPERATOR` window; it is the earlier of `(delivered_at + ttl_seconds)` and the policy decision's `Constraints.ttl_seconds` cap.

## §5 ApprovalBinding record

The `ApprovalBinding` is what `GRANTED` produces and what the Capability Runtime consumes. It is the durable receipt of operator consent. It is signed; it is single-use (for `EXACT_ACTION`); it is anchored in the evidence chain.

```proto
message ApprovalBinding {
  // Identity --------------------------------------------------------------
  string binding_id = 1;                       // "appb_" + 26-char ULID base32 (S0.1 §3.2)
  string approval_request_id = 2;              // backlink to ApprovalRequest

  // Bound action ----------------------------------------------------------
  string bound_action_request_id = 3;          // L3 ActionRequestId (EXACT_ACTION only)
  string bound_action_canonical_hash = 4;      // hex_lower(BLAKE3(JCS(action)))[:32]
  ApprovalBindingScope scope = 5;
  string bound_action_kind = 6;                // ACTION_FAMILY: dotted action name
  string bound_resource_class = 7;             // ACTION_FAMILY: resource family token

  // Subjects --------------------------------------------------------------
  string granting_subject_id = 8;              // S5.1 canonical subject id; SubjectKind = HUMAN_USER
  string co_signer_subject_id = 9;             // present iff scope.strength = DUAL
  ApprovalStrength strength = 10;

  // Timing ----------------------------------------------------------------
  google.protobuf.Timestamp granted_at = 11;
  google.protobuf.Timestamp expires_at = 12;
  ApprovalTtlClass ttl_class = 13;

  // Evidence anchoring ----------------------------------------------------
  string evidence_chain_root = 14;             // hash of prior evidence record at grant time
  string approval_request_hash = 15;           // request_hash from ApprovalRequest

  // Signatures ------------------------------------------------------------
  bytes signer_signature = 16;                 // Ed25519 over JCS(canonical binding fields 1..15)
  bytes co_signer_signature = 17;              // Ed25519 by co-signer; present iff DUAL
  string signing_key_id = 18;                  // identity service key that signed
}
```

Hash and identity conventions:

- `binding_id` follows the parallel `"appb_" + ULID + 26-char base32` convention (per S0.1 §3.2). The `appb_` prefix is deliberately distinct from the `apprq_` request-id prefix and from the legacy `appr_` approval-receipt prefix in S0.1; an `appb_` id can only appear after the FSM has issued a binding (`AWAITING_OPERATOR → GRANTED`), never before. The ULID's time component MUST equal the millisecond `granted_at` clock observed by the Approval Mechanics service.
- `bound_action_canonical_hash` is computed at grant time from the canonical (JCS) form of the bound action exactly as it existed when the operator saw the prompt. This is the **frozen** representation. The Capability Runtime will recompute the hash at `ExecuteAction` and compare; any divergence triggers §13 ACTION_REVISED.
- `evidence_chain_root` is the hash of the prior evidence record at grant time. It anchors the binding into the append-only chain (per [L9.1 §5](../L9_Observability_Admin_Operations/01_evidence_log.md)). A binding that does not carry a valid `evidence_chain_root` is rejected at consume time.

Signature rules:

- The canonical bytes signed are the JCS encoding of fields 1..15 (everything except `signer_signature`, `co_signer_signature`, and `signing_key_id`). The signing service is the L4 identity service ([L4.3 §11](./03_identity_model.md)) which holds the per-subject private key.
- For `DUAL` strength, both `signer_signature` and `co_signer_signature` are required; the canonical bytes are identical for both signers and both signatures must verify against the active identity bundle.
- A binding whose signature does not verify is treated as if it does not exist; the Capability Runtime fails the action closed and emits `APPROVAL_BINDING_VOIDED`.

Lifecycle:

- Issued: at the moment the FSM transitions `AWAITING_OPERATOR → GRANTED`.
- Spent: when the Capability Runtime consumes the binding for an `ExecuteAction` matching the binding's scope.
- Revoked: see §11.
- Voided: see §13 (action revision) and §10 (signature failure path).

## §6 FSM

The legal transitions of `ApprovalRequestState`. Any transition not listed here is forbidden; an attempt to drive the FSM through an illegal transition is a state-machine violation and emits `APPROVAL_BINDING_VOIDED` evidence with the request reference.

```text
                              created
                                 |
                                 v
                              DRAFT
                                 |
                          deliver surface
                                 |
                                 v
                          AWAITING_OPERATOR
                       /         |         \
              operator      TTL elapsed   delivery
              responds                    failed
              /     \           |           |
           GRANT    DENY        v           v
            |        |       EXPIRED   FAILED_DELIVERY
            v        v
        GRANTED   DENIED
       /    |
   consume  revoke
      |       |
      v       v
  CONSUMED  REVOKED
```

Allowed transitions, exhaustive:

| From                | To                  | Trigger                                                                           |
| ------------------- | ------------------- | --------------------------------------------------------------------------------- |
| `DRAFT`             | `AWAITING_OPERATOR` | Surface delivered to chosen channel; `delivered_at` set                           |
| `DRAFT`             | `FAILED_DELIVERY`   | No channel could deliver per §7                                                   |
| `DRAFT`             | `DENIED`            | TTL configuration invalid; pre-flight reject                                      |
| `AWAITING_OPERATOR` | `GRANTED`           | Operator (and co-signer if DUAL) submitted grant; signature verified              |
| `AWAITING_OPERATOR` | `DENIED`            | Operator submitted denial; reason = OPERATOR_REJECTED                             |
| `AWAITING_OPERATOR` | `EXPIRED`           | Wall-clock now ≥ `expires_at`; reason = TTL_EXPIRED                               |
| `AWAITING_OPERATOR` | `DENIED`            | Higher-priority request supersedes; reason = SUPERSEDED                           |
| `GRANTED`           | `CONSUMED`          | Capability Runtime consumed the binding for the bound action                      |
| `GRANTED`           | `REVOKED`           | Granting subject or higher-priority subject revoked; reason = REVOKED_BY_OPERATOR |
| `GRANTED`           | `DENIED`            | Action revised between grant and execute; reason = ACTION_REVISED                 |
| `GRANTED`           | `DENIED`            | Scope drift detected at consume time; reason = SCOPE_DRIFT                        |

Properties of the FSM:

- All terminal states (`DENIED`, `EXPIRED`, `REVOKED`, `CONSUMED`, `FAILED_DELIVERY`) emit exactly one closing evidence record (§10) before the request is sealed.
- The FSM is single-threaded per `approval_request_id`. Concurrent transitions on the same id are serialised through the Approval Mechanics service's per-id mutex; the second transition observes the post-state of the first and either no-ops or fails.
- A transition is recorded with the wall-clock time, the triggering subject (where applicable), and the prior evidence chain root.

## §7 Channel selection

The Approval Mechanics service selects exactly one `ApprovalChannel` per request. Selection is **deterministic** — given the operator session topology and the request's strength tier, the selected channel is a pure function. This determinism is required for replayability of evidence.

### §7.1 Inputs

The selector receives:

- The `Subject` of the proposing action (mostly informational; the approver is a different subject).
- The set of currently-active operator sessions (S5.1 `Session` records) whose `SubjectKind = HUMAN_USER`.
- The host's web exposure state (`LOOPBACK_ONLY` vs `LAN_ALLOWED`, per INV-006 / L4.1 §27 GPU/network bindings).
- The recovery-mode flag of the host.
- The required `ApprovalStrength` from the policy decision.

### §7.2 Selection order

The selector iterates the closed list below in order; the first rule whose predicate matches is the chosen channel.

```text
1. IF host.recovery_mode = true
   THEN channel = RECOVERY_CONSOLE
        AND require operator session with session_class = RECOVERY

2. IF an active HUMAN_USER session exists at the local KDE console
   AND session.session_class >= INTERACTIVE
   THEN channel = KDE_NATIVE_PROMPT

3. IF an active HUMAN_USER session exists on the local Web renderer
   AND host.web_exposure = LOOPBACK_ONLY
   AND session.session_class >= INTERACTIVE
   THEN channel = WEB_LOOPBACK_PROMPT

4. IF host.web_exposure = LAN_ALLOWED
   AND a WEB_EXPOSURE_GRANTED evidence record is current
   AND an active HUMAN_USER session exists on the LAN-bound Web renderer
   AND session.session_class >= STRONG
   THEN channel = WEB_LAN_PROMPT

5. IF an active HUMAN_USER session has a controlling TTY
   THEN channel = CLI_TTY_PROMPT

6. IF a bound mobile device exists for any HUMAN_USER member of the approver filter
   THEN channel = MOBILE_PUSH    (channel reserved → §7.3 fallback applies)

7. ELSE channel = NONE
   THEN state = FAILED_DELIVERY ; reason = DELIVERY_FAILED
```

### §7.3 Reserved channels

`MOBILE_PUSH` and `VOICE_CHALLENGE` are reserved in Rev.2: even when the selector would pick them, the wire format is undefined and the selector emits `FAILED_DELIVERY`. The selector logs the would-be channel in the failure evidence so that future revisions can audit how often the reserved channels were attempted.

### §7.4 Strength compatibility

A channel that cannot meet the requested `ApprovalStrength` is skipped. Concretely:

- `KDE_NATIVE_PROMPT`, `WEB_LOOPBACK_PROMPT`, `WEB_LAN_PROMPT`, `CLI_TTY_PROMPT` can carry `WEAK` and `STRONG`. They can carry `DUAL` only if the host has at least two distinct `HUMAN_USER` sessions active and the second can independently respond.
- `RECOVERY_CONSOLE` can carry any strength tier in recovery-mode requests.
- `MOBILE_PUSH` and `VOICE_CHALLENGE` are reserved regardless of strength.

If no channel matches the required strength, the request transitions `DRAFT → FAILED_DELIVERY`.

### §7.5 Operator-explicit override

A human operator may, through a separate dedicated control surface (`approval.routing.set_default_channel`), pin a default preference (e.g. always `KDE_NATIVE_PROMPT` on this host). The selector consults the pin **after** the recovery-mode rule but **before** the auto-selected order. The pin cannot select a reserved channel, cannot select `WEB_LAN_PROMPT` without `WEB_EXPOSURE_GRANTED`, and is itself a typed action that flows through the Policy Kernel.

## §8 TTL discipline

Every approval has a non-zero finite TTL. The recommended defaults table below maps an action's risk class — as carried in S0.1 `request.risk` flags and in the Policy Kernel's reason code — to an `ApprovalTtlClass`.

| Action risk class                           | Recommended `ApprovalTtlClass` | Hard upper bound |
| ------------------------------------------- | ------------------------------ | ---------------- |
| Low / reversible (read, status query)       | `TTL_SHORT`                    | 5 min            |
| Medium / state-changing but reversible      | `TTL_SHORT`                    | 5 min            |
| Privileged / non-trivial system effect      | `TTL_INSTANT`                  | 60 s             |
| Destructive / irreversible (delete, drop)   | `TTL_INSTANT`                  | 60 s             |
| Recovery-mode mutation                      | `TTL_RECOVERY`                 | 30 min           |
| Long-running batch under explicit clearance | `TTL_LONG`                     | 4 h              |
| Multi-step interactive workflow             | `TTL_MEDIUM`                   | 30 min           |

These are recommendations the Policy Kernel applies when its rule's `ApprovalRequirement.ttl_seconds` is omitted. A rule may explicitly request a different tier as long as the requested TTL is within the upper bound of that tier; requesting a TTL above the bound causes the bundle load to fail with `InvalidApprovalTtl`.

Constitutional anti-pattern: a rule that requests `ttl_seconds = 0` or any value above `TTL_LONG`'s bound is rejected at bundle load. There is no `TTL_INFINITE` tier; an "evergreen" approval is by construction impossible. This discipline binds INV-009 (approvals expire).

The TTL clock starts at `delivered_at`, not at `created_at`. A delay between policy decision and surface delivery does not consume the operator's TTL budget; the operator's window starts when the prompt is actually visible to them.

## §9 Trust surface contract

This section binds Approval Mechanics to the renderer trust surface defined in [L7.1 Surface Composition](../L7_Interaction_Renderers/01_surface_composition.md) and [L7.2 Shared UI Schema](../L7_Interaction_Renderers/02_shared_ui_schema.md).

### §9.1 NodeKind binding

The approval prompt is rendered as a UI tree whose root `NodeKind` is `APPROVAL_PROMPT` (L7.2 §3). No other `NodeKind` may carry approval semantics: a renderer that observes operator consent on a non-`APPROVAL_PROMPT` node treats the consent as untrusted and ignores it.

### §9.2 Composition zone binding

The `APPROVAL_PROMPT` root is composited in the **CHROME** zone (L7.1 §6, `CompositionZone.CHROME`). The renderer rejects any submission that places `APPROVAL_PROMPT` in `BACKGROUND`, `CONTENT`, or `OVERLAY` with `CompositionZoneForbidden` (L4.1 §27.2.1, INV-020/021). This is the same constitutional rule that protects every chrome surface; this sub-spec inherits it without alteration.

### §9.3 Authorship binding

The approval prompt tree is signed by the L4 identity service under the `_system:service:aios-chrome` subject. The rules:

- The tree's `is_ai_origin` flag is **always false** on every node. The tree-signing service overwrites the input value with `false` because the issuer is `_system:service:aios-chrome` whose `SubjectKind = SERVICE` (not `AI_AGENT`). This is the symmetric application of L7.2 §7.2 which sets `is_ai_origin = true` for AI-authored trees.
- The `is_trust_bearing` flag is `true` on the `APPROVAL_PROMPT` root.
- The signing service refuses to sign an approval-prompt tree if the issuer is `AI_AGENT`-classified (L7.2 I5). An AI subject cannot author an approval prompt under any circumstances — this is constitutional from L0 (INV-002 AI proposes never executes; INV-021 AI/human visual distinction; the renderer-side I5 in L7.2).

### §9.4 Visual distinction

L7.3 Visual Language is responsible for the actual visual treatment. This sub-spec only declares the structural flags that the visual language consumes; it does not specify colors, typography, or motion. A renderer compiled against the structural schema with no visual language attached produces a structurally correct but visually default approval surface.

### §9.5 Recovery aesthetic

When `host.recovery_mode = true`, the prompt is rendered with the recovery aesthetic (INV-022). The `recovery_only` flag on the `APPROVAL_PROMPT` root is set to `true`; L7.3 applies the recovery treatment. This sub-spec only sets the flag; visual semantics are L7.3.

## §10 Evidence

Approval Mechanics emits exactly the closed list of evidence record types below. Every record is appended through the [L9.1 Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md) `Append` RPC. Every record is in the standard hash chain (per S3.1 §5). Every record carries `subject`, `action_id`, `policy_decision_id`, and one of the approval-specific identifiers (`approval_request_id` or `binding_id`).

> **Closed-vocabulary discipline note.** The retention column below uses the closed `RetentionClass` enum (S3.1 §3): `STANDARD_24M` / `EXTENDED_60M` / `FOREVER`. Earlier drafts used `LONG (1 year)` informally; per S3.1 Wave 6 §25.2's translation, the LONG floor is mapped to `STANDARD_24M` (which exceeds the 1-year floor). All nine `APPROVAL_*` records carry `STANDARD_24M` here.

### §10.1 Closed record types emitted by this sub-spec

| Record type                | Emitted on                                                              | Default retention class         |
| -------------------------- | ----------------------------------------------------------------------- | ------------------------------- |
| `APPROVAL_REQUESTED`       | `created → DRAFT`                                                       | `STANDARD_24M` (≥ 1-year floor) |
| `APPROVAL_DELIVERED`       | `DRAFT → AWAITING_OPERATOR`                                             | `STANDARD_24M`                  |
| `APPROVAL_GRANTED`         | `AWAITING_OPERATOR → GRANTED`                                           | `STANDARD_24M`                  |
| `APPROVAL_DENIED`          | `AWAITING_OPERATOR → DENIED` (any non-TTL reason)                       | `STANDARD_24M`                  |
| `APPROVAL_EXPIRED`         | `AWAITING_OPERATOR → EXPIRED`                                           | `STANDARD_24M`                  |
| `APPROVAL_CONSUMED`        | `GRANTED → CONSUMED`                                                    | `STANDARD_24M`                  |
| `APPROVAL_REVOKED`         | `GRANTED → REVOKED`                                                     | `STANDARD_24M`                  |
| `APPROVAL_DELIVERY_FAILED` | `DRAFT → FAILED_DELIVERY`                                               | `STANDARD_24M`                  |
| `APPROVAL_BINDING_VOIDED`  | `GRANTED → DENIED(reason = ACTION_REVISED \| SCOPE_DRIFT \| signature)` | `STANDARD_24M`                  |

These nine record types are reserved values in the L9.1 `RecordType` enum. They extend the existing `APPROVAL_REQUESTED`, `APPROVAL_GRANTED`, `APPROVAL_DENIED` values (S3.1 §4) with six additional entries that this sub-spec contributes; bundle integration is queued for the next L9.1 RecordType bundle revision.

### §10.2 Record payloads

Each record's payload carries the minimum fields needed to reconstruct the FSM transition that emitted it, plus the prior evidence chain root for hash-chain continuity.

```proto
message ApprovalRequestedPayload {
  string approval_request_id = 1;
  string action_id = 2;
  string policy_decision_id = 3;
  string proposing_subject_id = 4;
  ApprovalStrength strength = 5;
  ApprovalBindingScope scope = 6;
  ApprovalTtlClass ttl_class = 7;
  uint32 ttl_seconds = 8;
  ApprovalChannel selected_channel = 9;
  string request_hash = 10;
}

message ApprovalGrantedPayload {
  string approval_request_id = 1;
  string binding_id = 2;
  string granting_subject_id = 3;
  string co_signer_subject_id = 4;
  ApprovalStrength strength = 5;
  string bound_action_canonical_hash = 6;
  google.protobuf.Timestamp expires_at = 7;
}

message ApprovalDeniedPayload {
  string approval_request_id = 1;
  ApprovalDenialReason reason = 2;
  string subject_who_denied = 3;       // empty if reason ∈ {TTL_EXPIRED, ACTION_REVISED, ...}
  string denial_message = 4;           // English plain-text; no secrets
}

message ApprovalConsumedPayload {
  string binding_id = 1;
  string bound_action_request_id = 2;
  string bound_action_canonical_hash = 3;
  google.protobuf.Timestamp consumed_at = 4;
}

message ApprovalRevokedPayload {
  string binding_id = 1;
  string revoker_subject_id = 2;
  string reason_message = 3;
}

message ApprovalDeliveryFailedPayload {
  string approval_request_id = 1;
  ApprovalChannel attempted_channel = 2;
  string failure_detail = 3;          // English plain-text; "no operator session"; never secrets
}

message ApprovalBindingVoidedPayload {
  string binding_id = 1;
  ApprovalDenialReason reason = 2;     // ACTION_REVISED | SCOPE_DRIFT | signature
  string previous_canonical_hash = 3;
  string current_canonical_hash = 4;   // observed hash that triggered the void
}
```

### §10.3 Retention

Default retention class is `STANDARD_24M` (24 months — meets the 1-year floor) for all approval-related records, drawn from the closed `RetentionClass` enum (S3.1 §3). A policy bundle may upgrade specific records to `EXTENDED_60M` or `FOREVER` retention through a constraint in the policy rule (e.g. for destructive actions on financial-tier groups). It cannot downgrade below `STANDARD_24M`; this sub-spec sets the floor. Per S3.1 Wave 6 §25.2, the historical `LONG (1 year)` floor is deliberately translated to `STANDARD_24M` to keep the closed enum exhaustive and to give every approval record at least 24 months of audit retention.

### §10.4 No secret leakage

INV-015 (evidence never contains secrets) binds every payload above. The `denial_message`, `failure_detail`, and `reason_message` fields are plain-text English by contract; they are reviewed by the L9 redaction profile and are subject to redaction at write time. The Approval Mechanics service does not include any field whose value class is `SECRET` or `PRIVATE_HIGH` in any evidence payload.

## §11 Revocation

A `GRANTED` binding may be revoked before `CONSUMED`. Revocation is a typed action that flows through the Policy Kernel like any other; the action's authority must be at least as strong as the original grant.

### §11.1 Who can revoke

- The `granting_subject_id` (self-revoke). Always allowed for self-issued bindings.
- For `DUAL` bindings, either the `granting_subject_id` or the `co_signer_subject_id` may revoke; revocation by either is sufficient to void the binding.
- A higher-priority subject — concretely, a `_system:local:operator-<id>` recovery operator or a group admin (group-tier dependent) — may revoke any binding under their authority.

### §11.2 How revocation is requested

The revoker submits an action `aiosfs.approval.revoke` with target `{approval_binding_id}`. The Policy Kernel evaluates the action under the standard pipeline; if approved, the action is executed by the Approval Mechanics service which transitions `GRANTED → REVOKED` and emits `APPROVAL_REVOKED`.

### §11.3 Race with consume

If the Capability Runtime initiated `ExecuteAction` consuming the binding before the revocation transition completed, the consume wins (it landed first in the per-id serial). The revocation request transitions to a `NoOpRevoked` end-state — a record is still emitted (`APPROVAL_REVOKED` with `revocation_observed_after_consume = true`), but the action's execution is not unwound. Rolling back the action's effects is the L3 rollback path, not approval revocation.

### §11.4 Bundle flip

A policy bundle flip during a `GRANTED` window does not auto-revoke. The binding finishes on its original bundle version, mirroring L4.1 §12.4 in-flight semantics. An operator who wants to invalidate active bindings on a bundle change must explicitly revoke each binding.

## §12 Dual control

`DUAL` strength requires two distinct human subjects to independently grant. This section specifies the discipline.

### §12.1 Co-signer constraints

- Both `granting_subject_id` and `co_signer_subject_id` MUST be `SubjectKind = HUMAN_USER` (S5.1 §3.1). An `AI_AGENT`, `SERVICE`, `APPLICATION`, `WORKFLOW`, `DEVICE`, or `REMOTE_OPERATOR` cannot serve as a DUAL co-signer.
- The two subjects MUST have distinct `canonical_subject_id`. The same human re-authenticating under a different membership does not count as two subjects (per S5.1 I4: a subject's canonical id is unique).
- Both subjects MUST be in the approver filter the policy decision specified.
- Both signatures MUST verify against the active identity bundle at grant time. Either signature failing to verify rejects the entire binding (`APPROVAL_BINDING_VOIDED` with reason = signature).

### §12.2 Independent prompts

The two co-signers receive **independent prompts** through the channel selector. Concretely: the selector runs once per signer and may select different channels for each. The Approval Mechanics service does not allow one human to drive both prompts on the same surface — the surfaces are bound to different `SessionId` values, and the L7 trust surface enforces session binding through the session signature in the UI tree.

### §12.3 Order independence

Either signer may grant first. The first grant transitions the FSM to a new intermediate state `AWAITING_CO_SIGNER`; the second grant transitions to `GRANTED`. (For schema simplicity the public FSM in §6 collapses this into `AWAITING_OPERATOR → GRANTED`; the intermediate state is internal to the Approval Mechanics service and is not exposed in evidence as a separate record type — only the two `APPROVAL_DELIVERED` records and the single `APPROVAL_GRANTED` record are emitted.)

### §12.4 TTL with dual control

The TTL is shared. The `expires_at` is computed once when the first prompt is delivered; both signers must respond within the same window. If the first signer grants and the second does not respond before `expires_at`, the FSM transitions `AWAITING_OPERATOR → EXPIRED` and the partial grant is discarded.

## §13 Anti-replay

A binding is single-use for `EXACT_ACTION` scope and `CONSUMED` is terminal. Re-presenting the same binding fails closed.

### §13.1 Single-use semantics

- The Capability Runtime, on `ExecuteAction`, requests the Approval Mechanics service to consume the binding atomically. The service performs the FSM transition `GRANTED → CONSUMED` under the per-id mutex. A second `ExecuteAction` against the same binding observes `CONSUMED` (terminal) and is rejected with `ApprovalAlreadyConsumed`.
- The Capability Runtime fails closed: an `ExecuteAction` whose presented binding is in any state other than `GRANTED` is rejected without side effects. No partial execution.
- The rejection emits a fresh evidence record with `record_type = APPROVAL_BINDING_VOIDED` and `denial_reason = ACTION_REVISED` (or the appropriate reason); the binding's terminal record is **not** rewritten because evidence is append-only (INV-005).

### §13.2 Action revision invariant

This is the constitutional anti-replay rule. Stated formally:

```text
INVARIANT (Action Revision):
  GIVEN a binding B with bound_action_canonical_hash = H_grant
  WHEN Capability Runtime invokes ExecuteAction with action A_exec
  THEN
    let H_exec = hex_lower(BLAKE3(JCS(A_exec)))[:32]
    IF H_exec != H_grant
    THEN
      transition B from GRANTED to DENIED with reason = ACTION_REVISED
      emit APPROVAL_BINDING_VOIDED
      reject ExecuteAction
```

The Capability Runtime is required to recompute the canonical hash at `ExecuteAction` and pass it to the consume call. The Approval Mechanics service performs the comparison server-side; clients cannot bypass the check by withholding the hash.

This rule binds INV-009 at the byte level: the bound action is precisely the action the operator saw, not "an action that looks like" the one the operator saw. A single-byte mutation in any payload field — including whitespace, key order, or normalization differences — voids the binding.

### §13.3 Scope drift for ACTION_FAMILY

`ACTION_FAMILY` scope binds to `(subject × action_kind × resource_class)`. At consume time, the Approval Mechanics service compares:

- `subject` of the bound action equals `granting_subject_id` (or, for a personal AI agent acting under operator approval, equals an explicitly-allowed delegation — out of scope for Rev.2).
- `action_kind` equals `bound_action_kind` exactly.
- `resource_class` equals `bound_resource_class` exactly. The resource class is the family token derived from the action's target by the policy kernel's enrichment (L4.1 §8); a mismatch is `SCOPE_DRIFT`.

A mismatch on any of the three voids the binding with reason = `SCOPE_DRIFT`.

### §13.4 Replay across bundle versions

A binding's `bundle_version` is recorded. If the bundle version active at consume time differs from the binding's `bundle_version`, the consume is allowed (per §11.4 in-flight semantics), but the consume evidence record carries both the binding's and the active bundle version so audit can reconstruct the policy context.

## §14 Hard-deny boundary

Hard-denied actions ([L4.1 §6](./01_policy_kernel.md)) cannot be approval-rescued. This sub-spec is explicit about the boundary.

### §14.1 Constitutional rule

```text
IF policy_decision.decision = DENY
   AND policy_decision.reason_code matches a hard-deny entry (§6)
THEN
  Approval Mechanics service rejects ApprovalRequest creation
  with code = HardDeniedNotApprovable
  AND emits an evidence record (record_type = APPROVAL_DENIED,
       reason = OPERATOR_REJECTED is wrong here — instead an explicit
       reason code HardDenyCannotBeApproved is recorded in
       denial_message)
```

In other words: there is no path through this sub-spec that converts a hard `DENY` into an `ALLOW`. The Capability Runtime never reaches Approval Mechanics for a hard-denied action because the Policy Kernel emits `DENY`, not `REQUIRE_APPROVAL`.

### §14.2 Emergency Override redirect

The only path to relax a specific scoped-`DENY` rule (not a hard deny) is the Emergency Override mechanism defined in [L4.5 Emergency Override](./05_emergency_override.md). Approval Mechanics is not Emergency Override; it does not implement override semantics; it does not produce override receipts; it does not bypass any rule. Any operator who attempts to use Approval Mechanics to override a hard deny is rejected at request creation, the rejection is evidence-logged, and the request is sealed.

The two sub-specs are complementary but disjoint: Approval Mechanics handles the routine case where policy says "ask the human"; Emergency Override handles the exceptional case where policy says "no" but the human, with stricter discipline, says "yes anyway, on the record".

### §14.3 AI self-approval prevention

L4.1 §17 prevents AI subjects from approving their own actions. This sub-spec inherits that invariant: the approver filter set by the Policy Kernel for any AI-proposed action excludes `AI_AGENT` `SubjectKind`; any binding whose `granting_subject_id` resolves to an `AI_AGENT` is rejected at signature verification (the identity service refuses to sign such a grant). This binding from S5.1 §10 (AI subject discipline) and L4.1 §17 (AI self-approval prevention) is what makes INV-002 enforceable through Approval Mechanics.

## §15 Cross-references

| Spec                                                                                    | Direction  | Relationship                                                                                                                                                                                                                          |
| --------------------------------------------------------------------------------------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [S0.1 Action Envelope + Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md) | consumer   | `request_hash` is reproduced into both `ApprovalRequest` and `ApprovalBinding`; canonical-form recomputation is performed at consume.                                                                                                 |
| [S2.3 Policy Kernel](./01_policy_kernel.md)                                             | consumer   | Receives `REQUIRE_APPROVAL` decisions and `ApprovalRequirement` parameters; emits the `ApprovalRequest`.                                                                                                                              |
| [S5.1 Identity Model](./03_identity_model.md)                                           | consumer   | Subject canonical form, `SubjectKind`, session class. Identity service signs the binding.                                                                                                                                             |
| [S5.2 Vault Broker](./02_vault_broker.md)                                               | constraint | A successful approval may be the trigger for a Vault capability issuance; capability binding API is defined in S5.2, not here.                                                                                                        |
| [S5.4 Emergency Override](./05_emergency_override.md)                                   | constraint | Disjoint mechanism; this sub-spec redirects hard-denied or scoped-`DENY` rescue requests to S5.4.                                                                                                                                     |
| [L7.1 Surface Composition](../L7_Interaction_Renderers/01_surface_composition.md)       | consumer   | `APPROVAL_PROMPT` is rendered in `CompositionZone.CHROME`.                                                                                                                                                                            |
| [L7.2 Shared UI Schema](../L7_Interaction_Renderers/02_shared_ui_schema.md)             | consumer   | `NodeKind = APPROVAL_PROMPT`, signed by `_system:service:aios-chrome`, `is_ai_origin = false`.                                                                                                                                        |
| [L7.3 Visual Language](../L7_Interaction_Renderers/03_visual_language.md)               | consumer   | Visual treatment of the prompt; `recovery_only` flag drives the recovery aesthetic.                                                                                                                                                   |
| [L9.1 Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)            | producer   | All nine `APPROVAL_*` record types appended through the L9.1 `Append` RPC; `STANDARD_24M` retention floor (per the closed `RetentionClass` enum; see S3.1 Wave 6 §25.2 for the LONG → STANDARD_24M translation).                      |
| [L0.4 Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md)     | enforcer   | Binds INV-002 (AI proposes never executes), INV-008 (default deny), INV-009 (approval bound and expiring), INV-010 (AI cannot self-approve), INV-015 (no secrets in evidence), INV-020/021 (chrome / AI vs human visual distinction). |

## §16 Worked examples

These examples are operational prose. They walk the FSM under three concrete scenarios and show what evidence is emitted at each step. Times are wall-clock examples; identifiers are illustrative.

### §16.1 Operator approves `aios.fs.write` from KDE — happy path

Setup. Group `family`. Human user `family:alice` is logged in at the local KDE console with `session_class = INTERACTIVE`. AI agent `family:family-assistant` proposes a write to an object under `groups/family/shared/notes/2026-05-09.md`. The action's risk flags include `privileged = true` (the object's privacy class is `INTERNAL`).

Step 1 — Policy decision. The Policy Kernel evaluates and returns `REQUIRE_APPROVAL` with `ApprovalRequirement{ required = true, approver_classes = ["human"], ttl_seconds = 300 }`. The decision id is `poldec_01HX...A1`. Reason code `AISelfApprovalPrevented` (L4.1 §17).

Step 2 — Request creation. The Capability Runtime hands the action plus the decision to the Approval Mechanics service. The service constructs `ApprovalRequest{ approval_request_id = apprq_01HX...B7, request_hash = <hash from S0.1>, scope = EXACT_ACTION, strength = STRONG, ttl_class = TTL_SHORT, ttl_seconds = 300, state = DRAFT }`. Evidence: `APPROVAL_REQUESTED` is appended to L9.1.

Step 3 — Channel selection. The selector runs §7.2: recovery-mode is false, alice has an active KDE session at `INTERACTIVE`. Rule 2 fires: `selected_channel = KDE_NATIVE_PROMPT`. The service constructs the UI tree with root `NodeKind = APPROVAL_PROMPT`, `is_trust_bearing = true`, `is_ai_origin = false`, signed by `_system:service:aios-chrome`. The KDE renderer accepts the tree, validates the composition zone is `CHROME`, and presents the prompt.

Step 4 — Delivery. The renderer sends a `delivered` ack. The service transitions `DRAFT → AWAITING_OPERATOR`, sets `delivered_at = T+250ms`, sets `expires_at = delivered_at + 300s`. Evidence: `APPROVAL_DELIVERED`.

Step 5 — Grant. Alice reads the prompt and presses Approve. The renderer collects the operator subject id (from her session signature) and submits to the Approval Mechanics service. The service verifies the session is still active, the subject is in the approver filter (`HUMAN_USER` ∈ `["human"]`), the `STRONG` strength is satisfied (her session class is `INTERACTIVE` — wait: `STRONG` strength requires `STRONG` session class. The service rejects with `SessionClassInsufficient`; alice is prompted for step-up reauthentication.) Alice authenticates with WebAuthn; her session is reissued with `session_class = STRONG`. Alice presses Approve again. The service verifies session class is now `STRONG`, computes `bound_action_canonical_hash` from the action she saw, constructs `ApprovalBinding{ binding_id = appb_01HX...C3, granting_subject_id = family:alice, signer_signature = <Ed25519> }`. The identity service signs the binding. The service transitions `AWAITING_OPERATOR → GRANTED`. Evidence: `APPROVAL_GRANTED`.

Step 6 — Consume. The Capability Runtime invokes `ExecuteAction` with the action and presents the binding. The service recomputes the canonical hash from the runtime's action; the hash matches the binding's `bound_action_canonical_hash`. The service transitions `GRANTED → CONSUMED`. Evidence: `APPROVAL_CONSUMED`.

Step 7 — Action proceeds. The Capability Runtime executes the write through the AIOS-FS adapter. Verification runs (S2.4). The action terminates `succeeded` per S0.1. The full evidence chain — `ACTION_RECEIVED → POLICY_DECISION → APPROVAL_REQUESTED → APPROVAL_DELIVERED → APPROVAL_GRANTED → APPROVAL_CONSUMED → EXECUTION_STARTED → EXECUTION_COMPLETED → VERIFICATION_RESULT` — is reconstructible from L9.1 by `correlation_id`.

### §16.2 Action revision case — agent submits, gets approval, payload mutates

Setup. Same group and subjects as §16.1. The AI agent proposes the same write but, between the moment alice grants and the moment the Capability Runtime calls `ExecuteAction`, the agent's planner mutates the action's payload (it adds a trailing newline to the body — a single-byte change).

Step 1–5 are identical to §16.1. The binding is GRANTED with `bound_action_canonical_hash = H_grant`.

Step 6 — Consume attempt. The Capability Runtime invokes `ExecuteAction` with the **mutated** action. The service recomputes the canonical hash: `H_exec ≠ H_grant`. The service applies §13.2: transition `GRANTED → DENIED` with `denial_reason = ACTION_REVISED`. Evidence: `APPROVAL_BINDING_VOIDED` with `previous_canonical_hash = H_grant`, `current_canonical_hash = H_exec`.

Step 7 — Capability Runtime rejects the action. The action transitions S0.1 lifecycle to `failed` with cause `ApprovalBindingVoided`. The agent is informed (through its action-status feedback channel, not through alice). Alice is not re-prompted automatically; the agent must re-propose the action, which produces a new `ApprovalRequest` with a new `request_hash`, which alice must approve again. There is no implicit recovery from a single-byte mutation.

This is the constitutional anti-replay rule in motion. It is annoying for agents, and that is the point: the action the operator approved is precisely the action that runs.

### §16.3 Dual-control delete on production data — STRONG + DUAL with two human subjects

Setup. Group `homelab`. The action is `aiosfs.recursive_delete` on a non-system path (so it is not hard-denied per L4.1 §6) but the policy bundle has a rule that requires `STRONG + DUAL` for any recursive delete on objects with `policy_tags = ["production"]`. Two human subjects are members of `homelab`: `homelab:alice` (admin) and `homelab:bob` (admin).

Step 1 — Policy decision. The kernel returns `REQUIRE_APPROVAL` with `ApprovalRequirement{ required = true, approver_classes = ["human"], require_human_co_signer = true, ttl_seconds = 60 }`. Strength is `STRONG`; scope `EXACT_ACTION`; ttl class `TTL_INSTANT`.

Step 2 — Request creation. `ApprovalRequest{ strength = STRONG, scope = EXACT_ACTION, require_co_signer = true, ttl_seconds = 60 }`. Evidence: `APPROVAL_REQUESTED`.

Step 3 — Channel selection. Both alice and bob have active sessions; alice on KDE, bob on Web loopback. The selector runs **twice** — once per signer. Result: alice receives `KDE_NATIVE_PROMPT`, bob receives `WEB_LOOPBACK_PROMPT`. Two independent surfaces, two distinct UI trees, two distinct session signatures.

Step 4 — Delivery. Both renderers ack. The shared `expires_at` is set to `min(alice_delivered_at, bob_delivered_at) + 60s`. Evidence: two `APPROVAL_DELIVERED` records.

Step 5 — Sequential grants. Alice grants first at T+8s. The internal state moves `AWAITING_OPERATOR → AWAITING_CO_SIGNER` (not externally observable). Bob grants at T+22s. The Approval Mechanics service collects both signatures, constructs `ApprovalBinding{ granting_subject_id = homelab:alice, co_signer_subject_id = homelab:bob, signer_signature, co_signer_signature }`. Both signatures verify against the active identity bundle. Transition `AWAITING_OPERATOR → GRANTED`. Single `APPROVAL_GRANTED` record (the public FSM collapses dual-control into one grant event for evidence simplicity, with both subject ids in the payload).

Step 6 — Consume. The Capability Runtime executes the recursive delete with the binding. Hash matches. `GRANTED → CONSUMED`. `APPROVAL_CONSUMED`.

Counter-example. If bob instead does not respond, alice's grant is held internally. At T+60s the FSM transitions `AWAITING_OPERATOR → EXPIRED`. `APPROVAL_EXPIRED` is emitted with a note that one of the two required signatures was missing. The recursive delete does not proceed. Bob receives no penalty; the design optimises for caution, not for fluency.

## §17 Open questions (deferred)

These items are intentionally out of scope for S5.3 and tracked elsewhere or queued for future revisions:

- **`BIOMETRIC` strength wire format** — channel-and-payload contract for biometric step-up. Currently reserved.
- **`MOBILE_PUSH` channel wire format** — push payload schema, secure-element binding, offline-capable proof. Currently reserved.
- **`VOICE_CHALLENGE` channel wire format** — spoken challenge phrase distribution, ASR confidence threshold, replay defenses. Currently reserved.
- **`SESSION_SCOPED` binding scope** — binding tied to a single Surface session; design needs careful interaction with S5.1 `Session` lifecycle. Currently reserved.
- **Cross-host approval delegation** — alice on host A approves an action proposed by an agent on host B. Requires multi-host identity federation, which is itself deferred per S5.1 §19.
- **Interactive batch approval** — approve N actions of the same kind in one prompt. Approximated by `ACTION_FAMILY` scope but a richer "approve list" UI is deferred.
- **Approval delegation chains** — alice authorises bob to approve on her behalf for a window. Out of scope; deferred to a future revision; needs careful consent and revocation semantics.
- **Adversarial robustness fixtures** — golden fixtures that audit FSM transitions under concurrent revoke/consume races, network-partitioned co-signer flows, and clock-skew TTL edge cases. Queued for the S5.3 acceptance harness.

## §18 Status & evidence grade

Status: REAL
Evidence: E1 (file exists; structural contract complete; signed off in spec rev.2 master index)

The next evidence step (E2) requires a typecheck-clean proto IDL extracted from this sub-spec into the `aios.approval.v1alpha1` schema package. The next step after (E3) requires unit and integration tests against the FSM and the channel selector. The next step (E4) requires end-to-end recovery and release-gate tests through a working renderer. The full E5 (live operational) status is reached only after the Approval Mechanics service is deployed and producing evidence in a non-simulation mode.

## See also

- [L4 Overview](./00_overview.md)
- [L4.1 Policy Kernel](./01_policy_kernel.md)
- [L4.2 Vault Broker](./02_vault_broker.md) (deferred)
- [L4.3 Identity Model](./03_identity_model.md)
- [L4.5 Emergency Override](./05_emergency_override.md) (deferred)
- [L0.4 Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md)
- [L7.1 Surface Composition](../L7_Interaction_Renderers/01_surface_composition.md)
- [L7.2 Shared UI Schema](../L7_Interaction_Renderers/02_shared_ui_schema.md)
- [L9.1 Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [Rev.1 §11 — Policy Kernel](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
