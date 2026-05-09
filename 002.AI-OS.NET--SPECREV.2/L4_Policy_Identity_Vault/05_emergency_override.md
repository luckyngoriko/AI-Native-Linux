# Emergency Override (Rev.2)

| Field          | Value                                                                                                   |
| -------------- | ------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists)                                         |
| Phase tag      | S5.4                                                                                                    |
| Layer          | L4 Policy, Identity, Vault                                                                              |
| Schema package | `aios.override.v1alpha1`                                                                                |
| Consumes       | S2.3 Policy Kernel hard-deny vocabulary, S5.1 Identity (HUMAN_USER subjects), S5.3 Approval mechanics   |
|                | (abstract), S5.2 Vault Broker (abstract), S3.1 Evidence Log RecordType + FOREVER retention, L1          |
|                | recovery boundary                                                                                       |
| Produces       | typed `OverrideRequest`/`OverrideBinding`, FSM, eight FOREVER-retained evidence record types, telemetry |

## §0 Reading order

Readers reviewing this spec for the first time are advised to read in the following order:

1. §1–§2 (purpose and disjointness from approval) for the framing.
2. §3 (closed vocabularies) for the schema-level shape.
3. §10 (non-overridable classes) and §14 (recovery special case) for the constitutional boundaries.
4. §6 (FSM) and §11 (cooldown and rate limits) for the operational shape.
5. §17 (worked examples) for concrete scenarios that exercise the rules.
6. §16.1 (adversarial robustness summary) for the threat model.
7. The remaining sections for the implementation-detail layer.

Implementers building the Override Manager should treat §3, §4, §5, §6, §7, §8, §9, §10, §11, §13 as the contract surface and the remaining sections as constraints on the surface. The integration checklist in §19.1 lists the upstream dependencies that must already be in place.

Auditors reviewing operational logs for override-related activity should treat §13 (evidence record types) and §17 (worked examples) as the canonical reference for what to expect in the chain. The eight FOREVER-retained record types are sufficient to reconstruct any override scenario: who requested, who confirmed, when it was granted, when it was consumed, when it expired, when it was revoked, and any post-hoc reviews. If a reconstruction shows a missing record (e.g. an `OVERRIDE_CONSUMED` without a preceding `OVERRIDE_GRANTED`), the chain is inconsistent — `CHAIN_INCONSISTENCY_DETECTED` per S3.1 §11.4 and the system is in degraded mode pending operator response.

Operators authorising overrides at request time should treat §10 (non-overridable classes), §11 (cooldown and rate limits), and §14 (recovery special case) as the rules they will be evaluated against. The override prompt itself surfaces the rule id and the strength tier, but the operator is responsible for understanding what they are authorising. The `justification_text` is the operator's record of that understanding; it is mandatory because the operator's reasoning is itself part of the audit witness.

Reviewers performing the periodic constitutional audit should treat the population of `OVERRIDE_*` records as one signal-set among many; the others are `RECOVERY_EVENT`, `TAMPER_DETECTED`, `INVARIANT_BUNDLE_LOADED`, `WEB_EXPOSURE_GRANTED`, and the FOREVER-retained subset of `POLICY_DECISION` denials. Together these surfaces describe every constitutionally significant event in the system's history. A spike in any one of them, or a pattern across several, is the kind of signal the constitutional audit is designed to detect.

The reading-order section is informative; the contractual surface is everything from §1 through §19. A reader who cannot follow the suggested order (for instance because they are reviewing a specific defect report) should at minimum confirm they have read the cross-references in §16 and the worked examples in §17 before reasoning about whether the contract is being honoured in a given scenario. A scenario that contradicts a worked example is a defect; a scenario that contradicts a cross-reference is a coordination problem; a scenario that contradicts both is a constitutional drift and warrants escalation to whoever owns the L4 surface.

Finally, this spec is intentionally written to be readable in one sitting by a non-implementer reviewer. The vocabulary is closed, the worked examples are concrete, and the integration checklist is short. If a reader finishes the file and is still unsure what an override is, what it costs, or what it cannot do, the spec has failed at its primary purpose and should be revised. Conversely, if a reader finishes the file confident that they could distinguish a legitimate override from an attempted bypass, the spec has succeeded.

The same readability principle is applied throughout the rev.2 contract bundle. S5.4 follows the same conventions as S2.3 (Policy Kernel), S5.1 (Identity Model), S3.1 (Evidence Log), and the L0 invariants catalog: closed enums in tables, FSM diagrams in code blocks, worked examples in numbered flows, cross-references in tables, and a final acceptance-criteria block. A reviewer who has read those upstream contracts will recognise the shape of this one.

A coda on terminology. The terms "constitution", "constitutional", "constitutionally fixed" appear throughout this spec. They are not metaphor; they refer to the closed set of rules that AIOS treats as inviolable until the system itself is rebuilt. The constitution is enumerated in L0 as the invariant catalog (`04_invariants.md`); the override path described here is the constitutionally-sanctioned pressure-relief valve for a small, explicit subset of policy decisions. The valve has weight (cooldown, rate caps, FOREVER evidence), pressure limits (TTL ceilings, scope constraints), and pressure-relief boundaries (`NonOverridableClass`). Used as designed, it lets a household repair its own AIOS without inviting an attacker to repair it for them.

End of spec body. The status header at the top of this file and the §19 grade ladder together carry the contract metadata. No further sections follow.

This file is sub-spec S5.4 of the AIOS rev.2 contract bundle. Its companions in L4 are S2.3 Policy Kernel (`01_policy_kernel.md`), S5.2 Vault Broker (`02_vault_broker.md`), S5.1 Identity Model (`03_identity_model.md`), and S5.3 Approval Mechanics (`04_approval_mechanics.md`). The L4 overview (`00_overview.md`) tracks the layer's headline status.

## §1 Purpose

The Policy Kernel (S2.3) hard-denies a small, constitutionally-fixed set of action classes. Hard-deny is the operating constitution speaking; ordinary approval cannot rescue it. There is, however, a single narrow path through which a hard-denied action may be executed: **Emergency Override**. This sub-spec defines that path.

Emergency Override exists for situations in which a scoped, non-constitutional hard-deny rule has fired — for instance the Policy Kernel refused to delete an old encrypted backup because retention policy hard-denies it — and a human operator (or two, or three) must intentionally and on the record carry out the action anyway. The mechanism is intentionally narrow, costly, audited at FOREVER retention, and reversible only forward in time. It is **not** a substitute for routine approval; it is a constitutional fire alarm.

The override path is reachable only by HUMAN_USER subjects (S5.1 §3). AI subjects, application subjects, service subjects, device subjects, workflow subjects, and remote-operator subjects without the HUMAN_USER kind are constitutionally barred from originating or confirming an override. The only relaxation of "two distinct humans on two distinct channels" is the recovery-boot special case (§14), and even there the requirement is "one human on a recovery console" — never zero humans, never an AI agent acting alone, never a service.

This file defines the closed override vocabulary, the request and binding records, the FSM, the quorum and channel-separation rules, the strength tiers, the TTL discipline, the constitutional list of classes that no override can rescue, the cooldown and rate limits, the trust-surface contract for the override prompt, the evidence record types (FOREVER), the recovery-mode special case, the reversibility-and-review rules, and the cross-references back to the specs that own approval, vault, identity, evidence, recovery boundary, and renderers.

Four constitutional invariants frame this entire spec. **INV-008 (Default deny)** is the upstream rule that produced the hard-deny in the first place — absence of an allow rule is denial, not implicit allow. **INV-002 (AI proposes, never executes)** is the rule that bars AI subjects from ever originating or confirming an override; emergency override is a HUMAN_USER-only path at the constitutional layer. **INV-005 (Evidence is append-only)** is the rule that prevents an override or a misuse of an override from being scrubbed from the historical record. **INV-014 (No proof, no completion)** is the rule that gives the previous three operational teeth: an override claim with no FOREVER-retained evidence record has no validity. The conjunction of INV-002 + INV-005 + INV-014 is the constitutional realisation of the principle this sub-spec exists to honor — that an override of a hard-deny is a constitutional event whose existence requires FOREVER evidence, whose claim has no validity without that evidence, and which an AI subject can never initiate. The four invariants together mean an override is always visible, always costly, never AI-originated, and never silent.

## §2 Scope and disjointness from approval

Emergency Override is **not** approval. The two paths are disjoint:

| Mechanism          | What it rescues                                | Who grants                                | Default cost   |
| ------------------ | ---------------------------------------------- | ----------------------------------------- | -------------- |
| Approval (S5.3)    | A `policy_pending` decision (REQUIRE_APPROVAL) | One subject (or co-signer) per S5.3 rules | Routine, cheap |
| Emergency Override | A `policy_denied` decision (hard-deny)         | 1, 2, or 3 distinct HUMAN_USER subjects   | Constitutional |

Approval rescues actions the Policy Kernel deems acceptable conditional on human consent. Override rescues actions the Policy Kernel has decided are categorically unacceptable. The cost asymmetry is the entire point. An override is louder, slower, and remembered FOREVER.

The two paths are also disjoint at the FSM layer. The action lifecycle (S0.1) distinguishes `policy_pending` (which approval rescues by transitioning to `approved`) from `policy_denied` (which override rescues by issuing a binding the Capability Runtime treats as if the policy decision had been ALLOW for the bound action only). An override never produces an approval record; an approval never produces an override binding. Conflating the two on emit is a `CHAIN_INCONSISTENCY_DETECTED` per S3.1 §11.4.

This spec **does not** redefine approval mechanics. Where it cites approval (e.g. ApprovalChannel for channel-separation), the citation is abstract: the actual contract for ApprovalChannel lives in S5.3 (`04_approval_mechanics.md`). Likewise where this spec references the vault's `ON_REVEAL_ONLY` flagging, the reference is abstract — the contract for that flag lives in S5.2 (`02_vault_broker.md`). This spec consumes those contracts but does not author them.

## §3 Vocabulary

All vocabularies in this section are closed. Adding a value is a versioned spec change. Bundle load fails on unknown values.

### §3.1 `OverrideState`

| Value                      | Meaning                                                                               |
| -------------------------- | ------------------------------------------------------------------------------------- |
| `OS_REQUESTED`             | An override request has been authored by a HUMAN_USER subject and is pending quorum   |
| `OS_AWAITING_DUAL_CONFIRM` | One or more confirming signatures are still required to satisfy the strength tier     |
| `OS_ACTIVE`                | Quorum + channel separation satisfied; an OverrideBinding has been issued and is live |
| `OS_CONSUMED`              | The bound action was executed under the override; terminal for ONE_ACTION scope       |
| `OS_EXPIRED`               | The TTL elapsed before consumption                                                    |
| `OS_REVOKED`               | A later override or operator action revoked an ACTIVE binding before consumption      |
| `OS_DENIED`                | The override request was refused (insufficient quorum, broad scope, non-overridable)  |

### §3.2 `OverrideStrength`

The strength tier names how many distinct HUMAN_USER subjects on how many distinct channels are required to grant the override. **WEAK / single-channel / single-subject solo overrides are constitutionally forbidden outside recovery mode.** There is no `WEAK` value in this enum. There is no `OPEN_ACCESS` value. There is no `BYPASS_QUORUM` value. The only solo path is `STRONG_SOLO` and it is gated to recovery boot.

| Value          | Quorum                                  | Channel rule                                             | Allowed contexts                           |
| -------------- | --------------------------------------- | -------------------------------------------------------- | ------------------------------------------ |
| `STRONG_SOLO`  | 1 HUMAN_USER subject with strong reauth | Single channel acceptable (recovery console)             | Recovery boot only (per L1 boundary)       |
| `DUAL_HUMAN`   | 2 distinct HUMAN_USER subjects          | Two distinct ApprovalChannel values, distinct sessions   | Default for non-recovery overrides         |
| `TRIPLE_HUMAN` | 3 distinct HUMAN_USER subjects          | Three distinct ApprovalChannel values, distinct sessions | The deepest non-constitutional hard-denies |

`DUAL_HUMAN` is the floor for any override outside recovery mode. The Policy Kernel and the Override Manager both reject any override with `strength = STRONG_SOLO` when `recovery_mode = false`.

### §3.3 `OverrideScope`

| Value             | Meaning                                                                                                  |
| ----------------- | -------------------------------------------------------------------------------------------------------- |
| `ONE_ACTION`      | The binding is bound to one exact ActionRequest (EXACT_ACTION binding); single CONSUMED is terminal      |
| `ONE_SUBJECT_TTL` | The binding allows one named subject to attempt one specific `action_kind` for a hard-capped TTL ≤ 5 min |

There is **no** `OPEN_SCOPE`. There is **no** `ALL_ACTIONS`. There is **no** `INDEFINITE`. There is **no** `BLANKET`. Override is per-action or per-narrow-window; nothing else exists.

`ONE_SUBJECT_TTL` is intended for incident response where the same hard-denied operation must be retried (e.g., during a fast policy DB restoration). It is **not** a workaround for `ONE_ACTION`'s narrowness; it is bounded by both the closed `action_kind` family and the 5-minute TTL ceiling.

The `action_kind` family is itself a closed concept. A family in this context is a single dotted action name from the L3 capability catalog (e.g. `policy.kernel.restore`). It is **not** a glob, not a prefix match, and not a regular expression. An override scoped to `ONE_SUBJECT_TTL` for `policy.kernel.restore` does not cover `policy.kernel.restart` or `policy.kernel.snapshot`. This is intentional: the narrowness of the family is what allows the slightly more permissive TTL window to remain constitutional.

### §3.4 `OverrideTtlClass`

| Value                   | Maximum lifetime | Allowed strengths             |
| ----------------------- | ---------------- | ----------------------------- |
| `TTL_OVERRIDE_INSTANT`  | ≤ 60 s           | `DUAL_HUMAN`, `TRIPLE_HUMAN`  |
| `TTL_OVERRIDE_SHORT`    | ≤ 5 min          | `DUAL_HUMAN`, `TRIPLE_HUMAN`  |
| `TTL_OVERRIDE_RECOVERY` | ≤ 15 min         | `STRONG_SOLO` (recovery only) |

No tier exceeds 15 minutes. No infinite TTL exists. `expires_at` is computed at the moment of grant, not request.

### §3.5 `OverrideDenialReason`

| Value                    | When emitted                                                                             |
| ------------------------ | ---------------------------------------------------------------------------------------- |
| `INSUFFICIENT_QUORUM`    | Required confirming subjects did not arrive within the request's confirm window          |
| `TTL_EXPIRED`            | The request's confirm window or the binding's TTL elapsed                                |
| `SCOPE_TOO_BROAD`        | The requested scope or `action_kind` family exceeds `ONE_ACTION`/`ONE_SUBJECT_TTL` rules |
| `TARGET_NOT_OVERRIDABLE` | The target hard-deny is in `NonOverridableClass` (§10)                                   |
| `CHANNEL_UNAVAILABLE`    | A confirming channel is offline / unreachable; quorum cannot be satisfied                |
| `SUBJECT_NOT_HUMAN`      | A confirming subject's `kind` is not `HUMAN_USER`                                        |
| `CO_SUBJECT_IDENTICAL`   | Two grants arrived from the same subject, same session, or same channel                  |
| `REVOKED`                | The request or binding was revoked before consumption                                    |

### §3.6 `OverrideRateClass`

| Value               | Per-subject monthly cap | Default for                            |
| ------------------- | ----------------------- | -------------------------------------- |
| `LIMITED_LOW`       | ≤ 3 / month             | All HUMAN_USER subjects by default     |
| `LIMITED_MED`       | ≤ 10 / month            | Operators with explicit elevated grant |
| `RECOVERY_OPERATOR` | ≤ 30 / month            | `_system` recovery operators only      |

The default rate class is `LIMITED_LOW`. Promotion to `LIMITED_MED` or `RECOVERY_OPERATOR` is itself a system mutation per S2.3 §26.2.2 (`RecoveryRequiredForSystemMutation`) and S5.1 §7.

## §4 OverrideRequest record

An OverrideRequest is the artifact authored by the requesting subject. It is the input to the FSM; until quorum is met it remains in `OS_REQUESTED` or `OS_AWAITING_DUAL_CONFIRM`.

```proto
syntax = "proto3";
package aios.override.v1alpha1;

import "google/protobuf/timestamp.proto";

message OverrideRequest {
  string override_request_id = 1;             // "ovrq_" + 26-char base32 ULID body (S0.1 §3.2)
  string requesting_subject_id = 2;           // canonical subject id, kind = HUMAN_USER
  string target_action_request_id = 3;        // the ActionRequest being overridden
  string target_action_canonical_hash = 4;    // hex_lower(BLAKE3(JCS(action)))[:32]
  string target_hard_deny_rule_id = 5;        // S2.3 hard-deny rule id, e.g. "hd.privacy_class_downgrade"
  OverrideStrength strength = 6;
  OverrideScope scope = 7;
  OverrideTtlClass ttl_class = 8;
  string action_kind_family = 9;              // populated only when scope = ONE_SUBJECT_TTL
  string target_subject_id = 10;              // populated only when scope = ONE_SUBJECT_TTL
  string justification_text = 11;             // ≥ 32 chars, sanitized for secrets
  string evidence_chain_root_at_request = 12; // hash of evidence head at request time
  google.protobuf.Timestamp requested_at = 13;
  google.protobuf.Timestamp confirm_window_expires_at = 14;
  bytes requesting_subject_signature = 15;    // Ed25519 over canonical JCS of fields 1..14
  string identity_bundle_version = 16;
}
```

Field discipline:

- `target_hard_deny_rule_id` MUST identify exactly one rule from the S2.3 hard-deny list. The Override Manager looks up that rule in the active policy bundle; if the rule is in `NonOverridableClass` (§10), the request is immediately denied with `TARGET_NOT_OVERRIDABLE` and an `OVERRIDE_DENIED` evidence record is written.
- `target_action_canonical_hash` MUST equal the canonical hash of the bound action. This is the same hash the Policy Kernel used in its decision (S2.3 §4 `request_hash`). Mutation of the action invalidates the request.
- `justification_text` MUST be ≥ 32 ASCII characters. The text is recorded in evidence (after secret-shaped redaction per S3.1 §14 default profile). The text length minimum is constitutional; an empty or trivial justification is itself a denial.
- `evidence_chain_root_at_request` ties the request to the evidence head at request time. The grant record (§5) carries `evidence_chain_root_at_grant`; a successful grant proves the head moved forward.
- `requesting_subject_signature` is verified by the Override Manager before the FSM advances. A forged signature is a `SUBJECT_SIGNATURE_FAILURE` per S5.1 §14.1.

The request id format is `ovrq_` prefix + 26-char base32 ULID body (per S0.1 §3.2 prefix-namespace registry). The grant record (§5) uses the parallel `ovr_` prefix; both prefixes use underscore as the separator, matching the project-wide convention.

## §5 OverrideBinding record

An OverrideBinding is what the Override Manager issues once quorum and channel separation are satisfied. It is the artifact the Capability Runtime consults when it would otherwise hard-deny the bound action.

```proto
message OverrideBinding {
  string override_id = 1;                      // "ovr_" + 26-char base32 ULID body (S0.1 §3.2)
  string source_request_id = 2;                // the OverrideRequest that produced this
  string target_action_request_id = 3;
  string target_action_canonical_hash = 4;     // hex_lower(BLAKE3(JCS(action)))[:32]
  string target_hard_deny_rule_id = 5;
  string requesting_subject_id = 6;
  repeated string confirming_subject_ids = 7;  // length 1, 2, or 3 matching strength
  OverrideStrength strength = 8;
  OverrideScope scope = 9;
  OverrideTtlClass ttl_class = 10;
  string action_kind_family = 11;
  string target_subject_id = 12;
  google.protobuf.Timestamp granted_at = 13;
  google.protobuf.Timestamp expires_at = 14;
  string evidence_chain_root_at_request = 15;
  string evidence_chain_root_at_grant = 16;    // MUST be a later head than _at_request
  string justification_text = 17;              // mirrored from request, sanitized
  repeated bytes confirming_signatures = 18;   // Ed25519 over canonical JCS, one per confirming subject
  string identity_bundle_version = 19;
  bytes issuer_signature = 20;                 // Override Manager's Ed25519 over fields 1..19
}
```

Field discipline:

- `confirming_subject_ids.length` MUST equal `1` for `STRONG_SOLO`, `2` for `DUAL_HUMAN`, `3` for `TRIPLE_HUMAN`. Mismatch is `INSUFFICIENT_QUORUM`.
- The confirming list MUST be a set: no duplicates, no `requesting_subject_id` reappearing, no `_system` synthesised entries. Duplicates are `CO_SUBJECT_IDENTICAL`.
- All confirming subjects MUST have `kind = HUMAN_USER` per S5.1 §3. Anything else is `SUBJECT_NOT_HUMAN`.
- `confirming_signatures.length` MUST equal `confirming_subject_ids.length`; signatures are verified individually.
- `evidence_chain_root_at_grant` MUST be strictly later than `evidence_chain_root_at_request`. The hash chain (S3.1 §5) makes the ordering verifiable. Equal hashes are a `CHAIN_INCONSISTENCY_DETECTED` per S3.1 §11.4.
- `expires_at` is computed at grant time as `granted_at + ttl_class_max`. Override Manager rejects requests where the implied `expires_at` would exceed the ttl_class ceiling.
- `issuer_signature` is the Override Manager's Ed25519 signature over the canonical JCS of fields 1 through 19. The Capability Runtime verifies this signature before honouring the binding; signature failure is `SUBJECT_SIGNATURE_FAILURE` (treated as forgery, FOREVER evidence).

The binding id format is `ovr_` prefix + 26-char base32 ULID body — distinct from the request id prefix `ovrq_` so the two cannot be confused in logs. Both prefixes are registered in S0.1 §3.2.

Canonical hashing convention. Every hash field in this spec uses the project-wide convention `hex_lower(BLAKE3(JCS(<value>)))[:32]`: BLAKE3 over the JCS canonicalisation of the value, lowercase hex, truncated to the first 32 hex characters. This matches S0.1 §8.5 and the convention used by S2.3 for `request_hash` and by S3.1 for evidence-receipt hashing. Truncation to 128 bits is sufficient for collision resistance at the population sizes AIOS handles; full BLAKE3 output is reserved for chunk identities (S1.3) where storage handles persist for the lifetime of the system.

Why two distinct id prefixes. A common implementation mistake in audit systems is to emit the same id for "the operator asked for X" and "the operator was granted X". When the audit trail is later reviewed, the ambiguity makes it impossible to tell whether a signed grant was always present or was synthesised after the fact. The `ovrq_` / `ovr_` split forecloses that ambiguity at the schema level: an `ovr_` id can only appear after the FSM has issued a binding, never before.

## §6 FSM

```text
            authoring                     quorum                       use
                |                             |                          |
                v                             v                          v
        +---------------+              +-------------+            +-----------+
        | OS_REQUESTED  |---confirm-->| OS_AWAITING |---grant-->| OS_ACTIVE |---consume-->| OS_CONSUMED |
        +---------------+              | DUAL_CONFIRM|            +-----------+
                |                       +-------------+                |
                |                             |                          |--TTL elapsed-->| OS_EXPIRED |
                |--non-overridable-->| OS_DENIED |                       |
                |--quorum window     +-----------+                       |--operator/system-->| OS_REVOKED |
                |     elapsed         ^
                |                     |
                |--scope too broad----+
                |
                +--subject_not_human--+
                |
                +--co_subject_identical-+
```

Allowed transitions:

| From                       | To                         | Trigger                                                                |
| -------------------------- | -------------------------- | ---------------------------------------------------------------------- |
| `OS_REQUESTED`             | `OS_AWAITING_DUAL_CONFIRM` | First confirmation arrives but quorum is not yet met                   |
| `OS_REQUESTED`             | `OS_ACTIVE`                | Strength is `STRONG_SOLO` and recovery_mode = true (single confirm)    |
| `OS_REQUESTED`             | `OS_DENIED`                | Target rule in `NonOverridableClass`, or scope/strength rule violation |
| `OS_AWAITING_DUAL_CONFIRM` | `OS_ACTIVE`                | All required confirmations satisfied with channel separation           |
| `OS_AWAITING_DUAL_CONFIRM` | `OS_DENIED`                | Confirm window expired or `CO_SUBJECT_IDENTICAL` detected              |
| `OS_ACTIVE`                | `OS_CONSUMED`              | Capability Runtime executes the bound action                           |
| `OS_ACTIVE`                | `OS_EXPIRED`               | TTL elapsed before consumption                                         |
| `OS_ACTIVE`                | `OS_REVOKED`               | Operator or system revocation prior to consumption                     |

Forbidden transitions (every forbidden transition is itself an `OVERRIDE_DENIED` or `CHAIN_INCONSISTENCY_DETECTED` event):

- `OS_CONSUMED` → anything (terminal; physical-world effects cannot be unwound by FSM)
- `OS_EXPIRED` → anything
- `OS_REVOKED` → anything
- `OS_DENIED` → anything except permanent retention as evidence
- `OS_ACTIVE` → `OS_ACTIVE` (no re-grant; new override required)

State persistence. The FSM state lives in the Override Manager's authoritative AIOS-FS object (per S1.3 transactional semantics). Crash recovery preserves the in-flight state: a request in `OS_AWAITING_DUAL_CONFIRM` at crash time resumes in the same state at boot, with the confirm window unchanged (the Override Manager uses wall-clock timestamps, not steady-clock, for window expiry). A binding in `OS_ACTIVE` at crash time resumes valid; consumption events that did not produce evidence before the crash are lost and the binding remains `OS_ACTIVE` until either the TTL elapses (`OS_EXPIRED`) or a fresh consumption attempt completes successfully (`OS_CONSUMED`).

Idempotency. The FSM is idempotent across retries at the protocol level. A duplicate `RequestOverride` RPC for the same `(target_action_request_id, target_action_canonical_hash, requesting_subject_id)` triple within a 5-minute window returns the existing `override_request_id` rather than creating a second request. Likewise a duplicate `Confirm` for an already-confirmed `(override_request_id, confirming_subject_id)` is a no-op rather than a `CO_SUBJECT_IDENTICAL` denial. This idempotency does not relax the constitutional checks; it just protects against benign retries by clients on flaky networks.

Concurrent confirmation. Two confirming subjects may sign simultaneously. The Override Manager serializes confirms via the AIOS-FS pointer-CAS protocol (S1.3 §6); whichever signing event lands first becomes the second-confirm-of-three (or first-of-two), and the other becomes the third (or second). Both signing events are recorded as separate `OVERRIDE_QUORUM_RECEIVED` events in the order of CAS success.

Race against TTL. If the Capability Runtime begins executing the bound action at `T = expires_at - 1s` and execution takes 5 seconds, the binding is in a window where TTL has elapsed but consumption is still in progress. The Override Manager treats execution start as the consumption event for FSM purposes: if `EXECUTION_STARTED` (S3.1 §4) was emitted under a still-active binding, the FSM transitions to `OS_CONSUMED` at the moment of `EXECUTION_STARTED`, and any `OS_EXPIRED` evidence emission is a CHAIN_INCONSISTENCY for the same `override_id`. The TTL is therefore "TTL to start consumption", not "TTL to finish consumption".

## §7 Quorum and channel separation

Channel separation is a defence-in-depth rule. A single compromised laptop, browser session, or kiosk must not be able to issue an override on its own. The Override Manager therefore demands:

1. Each confirming subject's grant arrives on a **distinct ApprovalChannel** value (closed enum owned abstractly by S5.3).
2. Each confirming subject is in a **distinct active session** (S5.1 §8).
3. No two confirming subject ids are equal.

Two grants on the same channel from the same session are a `CO_SUBJECT_IDENTICAL` denial. Two grants from the same canonical subject on different channels are also `CO_SUBJECT_IDENTICAL` (subject distinctness wins; channel distinctness is necessary but not sufficient).

ApprovalChannel itself is **referenced abstractly**. This sub-spec does not enumerate ApprovalChannel values or define their bindings; that contract belongs to S5.3 (`04_approval_mechanics.md`). The constraint here is structural: whatever ApprovalChannel values S5.3 closes, the Override Manager treats them as opaque distinguishers.

Channel-availability failures are `CHANNEL_UNAVAILABLE` denials. The Override Manager does **not** silently degrade quorum from `DUAL_HUMAN` to `STRONG_SOLO` when a channel is offline; degradation requires explicit reauthoring as a different request. There is no "channel fallback" mechanism. If KDE is down and Web is down, the override fails. The only way to escalate is to physically gain access to a working channel — which is the entire intent.

Why this matters. The threat model that motivates channel separation is "one machine compromised". An attacker who controls Admin A's KDE session must not be able to also produce a signed grant from "Admin B" on the same KDE session, even if they have somehow obtained Admin B's credentials. By requiring that the two grants land on two different ApprovalChannel values, the attacker would have to compromise two distinct channels — which in a well-deployed AIOS means two distinct trust paths (e.g. native KDE + Web from a phone, or KDE + a hardware-token signing dialog).

Subject distinctness vs channel distinctness. A common subtlety: subject distinctness is necessary even when channel distinctness is satisfied. Admin A signing once on KDE_LOCAL and once on WEB_LOCAL from two browsers does **not** satisfy `DUAL_HUMAN`. The Override Manager checks subject distinctness first, then channel distinctness, then session distinctness; failure on any axis is `CO_SUBJECT_IDENTICAL` regardless of which channels were used.

## §8 Strength tiers

The Override Manager picks the **minimum** acceptable strength tier for a given target, using this constitutional table:

| Target hard-deny class                                             | Minimum strength        | Notes                                          |
| ------------------------------------------------------------------ | ----------------------- | ---------------------------------------------- |
| Recovery-mode operations (e.g. `hd.modify_boot_chain` in recovery) | `STRONG_SOLO`           | Permitted only when L1 recovery boot is active |
| Routine scoped hard-denies (e.g. retention-blocked deletion)       | `DUAL_HUMAN`            | Default for non-recovery cases                 |
| Deep non-constitutional hard-denies (e.g. policy log compaction)   | `TRIPLE_HUMAN`          | Three distinct humans, three distinct channels |
| Anything in `NonOverridableClass` (§10)                            | (none — request denied) | Hard-constitutional, not overridable           |

A request authored with a strength tier **lower** than the minimum is rejected with `INSUFFICIENT_QUORUM`. A request with a higher tier than required is permitted (callers may always choose a stronger discipline).

Strength tiers cannot be reduced mid-flight. If a confirming subject drops out, the request remains in `OS_AWAITING_DUAL_CONFIRM` until either the confirm window elapses (then `OS_DENIED`/`TTL_EXPIRED`) or another confirming subject signs — the strength tier authored at request time is the strength tier the FSM enforces.

Strength tiers can be increased mid-flight via reauthoring as a fresh request. If the operator initially authored `DUAL_HUMAN` and then realises the target hard-deny demands `TRIPLE_HUMAN`, the existing request is allowed to expire (or revoked by the originator) and a new request is authored at the higher tier. The two requests are entirely separate from the FSM's perspective; the second request does not inherit signatures from the first.

The minimum-strength table above is constitutionally fixed and lives in the Override Manager's compiled-in policy, not in a runtime-loadable bundle. A policy bundle attempting to lower the minimum (for example by mapping a deep hard-deny to `DUAL_HUMAN` instead of `TRIPLE_HUMAN`) is rejected at bundle load with `InvariantLooseningAttempted` per S2.3 §27.5.

## §9 TTL discipline

Three discipline rules, all constitutional:

1. **No infinite TTL.** Every `OverrideTtlClass` carries a hard ceiling: `TTL_OVERRIDE_INSTANT ≤ 60 s`, `TTL_OVERRIDE_SHORT ≤ 5 min`, `TTL_OVERRIDE_RECOVERY ≤ 15 min`. Requests authoring a longer ceiling are rejected at parse time.
2. **Max ceiling is 15 minutes.** Even in recovery mode, the TTL cannot exceed 15 minutes. This is the hard wall above which an override stops being "emergency" and starts being "policy".
3. **TTL is computed at grant.** `expires_at = granted_at + ttl_class_max`. Authoring time does not consume the TTL; the clock starts when the binding becomes `OS_ACTIVE`.

`ONE_SUBJECT_TTL` scope adds a fourth rule: the TTL ceiling is the more restrictive of the `OverrideTtlClass` ceiling and 5 minutes. `ONE_SUBJECT_TTL` cannot be combined with `TTL_OVERRIDE_RECOVERY`'s 15-minute ceiling.

A fifth rule worth stating explicitly: TTL is one-way. The Override Manager does not extend a binding's TTL after grant. An operator who needs more time must allow the current binding to expire, allow the cooldown window to elapse, and author a fresh request. This is a deliberate cost: extending TTL would amount to a quiet doubling of the override's window without re-establishing quorum, which would weaken every other discipline in this spec.

Clock authority. `granted_at` and `expires_at` use the Override Manager's wall clock, which is the same clock S3.1 §11.2 declares server-authoritative for evidence timestamps. Clock skew between the requesting subject's host and the Override Manager is not relevant to TTL; only the Override Manager's clock counts. If the Override Manager's clock goes backwards across a restart, all `OS_ACTIVE` bindings are revoked with `OS_REVOKED` and a `RECOVERY_EVENT` is emitted per S3.1 §11.2's clock-rewind handling.

## §10 Non-overridable classes

The `NonOverridableClass` enum is the constitutional list of action classes that **no override can rescue**. No quorum, no TTL, no recovery path overrides them. The only way to change them is to rebuild the system with new policy rules and a new evidence-chain root — an operation that is **out of scope for runtime** and treated as installing a new AIOS.

```proto
enum NonOverridableClass {
  NON_OVERRIDABLE_CLASS_UNSPECIFIED = 0;
  AIOS_RECOVERY_BREAK         = 1;  // disabling the recovery boundary
  EVIDENCE_LOG_REWRITE        = 2;  // modifying or truncating evidence prior to current head
  VAULT_RAW_REVEAL_BYPASS     = 3;  // bypassing the use-without-reveal contract for material flagged ON_REVEAL_ONLY in the vault (S5.2)
  IDENTITY_KEY_FORGE          = 4;  // minting a Subject without proper key generation path
  POLICY_DENY_LIST_DELETE     = 5;  // silently dropping rules from the deny list
  AIOS_CHROME_REPLACEMENT     = 6;  // installing non-AIOS chrome at the trust path
}
```

Citations and rationale:

- `AIOS_RECOVERY_BREAK` binds INV-001 (recovery independent of L5) and INV-004 (recovery boundary). An override that disables recovery would brick the system; the constitution refuses.
- `EVIDENCE_LOG_REWRITE` binds INV-005 (evidence append-only) and S3.1 §11.5 tamper response. Rewriting evidence prior to current head is the constitutional definition of tampering.
- `VAULT_RAW_REVEAL_BYPASS` binds INV-003 (secrets are capabilities) and INV-018 (vault never leaks raw secrets). The vault's `ON_REVEAL_ONLY` flag (referenced abstractly into S5.2) marks material that even the broker's reveal-to-human path is forbidden to touch.
- `IDENTITY_KEY_FORGE` binds S5.1 §I1 (Subject is unforgeable). Any subject minted outside the proper key-generation path is a forgery; an override that allowed it would invalidate the entire identity model.
- `POLICY_DENY_LIST_DELETE` binds INV-008 (default deny) and the conjunction of INV-005 (evidence append-only) + INV-014 (no proof, no completion) which jointly enforce that any change to the deny list is a constitutional event requiring FOREVER evidence; silent deletion of deny-list entries would degrade the constitution undetectably.
- `AIOS_CHROME_REPLACEMENT` binds INV-019 (visual identity preserved), INV-020 (trust indicators visible), and INV-022 (recovery aesthetic distinct). An override that swapped AIOS chrome for non-AIOS chrome would convert the OS into a phishing surface.

Operational consequence: an override request whose `target_hard_deny_rule_id` resolves to any rule whose policy class is in `NonOverridableClass` is **immediately** denied with `TARGET_NOT_OVERRIDABLE`. The denial itself is recorded as `OVERRIDE_DENIED` at FOREVER retention (§13) and triggers an `OVERRIDE_REVIEW` record so that the very attempt is forensically visible — even an attempted bypass is part of the audit witness.

The principle "hard-deny override is never silent" — jointly enforced by INV-002 (AI proposes, never executes), INV-005 (evidence append-only), and INV-014 (no proof, no completion) — is satisfied here in two senses: (1) overrides are loud, costly, and FOREVER-retained, so an override claim without FOREVER evidence has no constitutional validity (INV-005 + INV-014), and AI subjects cannot originate the override action at all (INV-002); (2) `NonOverridableClass` overrides are not possible regardless of strength, channel, quorum, or subject.

A note on minimality. The six classes in `NonOverridableClass` are deliberately the smallest set that preserves the AIOS constitution. They are not "everything dangerous"; they are "everything whose breach would invalidate the audit-and-recovery model that makes everything else safe." A retention-violation deletion is dangerous but recoverable from backup and audited from `OVERRIDE_CONSUMED`; a malicious recovery-boundary disablement is unrecoverable and silently lethal. The asymmetry — between recoverable-and-audited dangers and unrecoverable-and-silencing dangers — is what determines class membership. A future revision that proposes to add a class to `NonOverridableClass` must show that the proposed class is both unrecoverable and silencing; otherwise the class belongs in the routine hard-deny set, not in the constitutional non-overridable set.

A note on stability. The six classes are also chosen for stability under realistic implementation drift. As AIOS gains features (new sandboxes, new renderers, new GPU paths, new identity types), each new feature introduces new hard-deny rules in S2.3. The mapping of those rules to `NonOverridableClass` membership is performed in S2.3, not here; this spec just enumerates the classes. The number of classes can grow over time, but the discipline that each class be unrecoverable-and-silencing keeps the growth bounded.

## §11 Cooldown and rate limits

**Per-subject cooldown.** After any successful override (`OS_CONSUMED`), the system enters `OVERRIDE_COOLDOWN` for the requesting subject for ≥ 5 minutes. During cooldown:

- Any override request from that subject is denied with `INSUFFICIENT_QUORUM` (rate-limit class).
- An `OVERRIDE_DENIED` record is emitted at FOREVER retention (§13) with reason `INSUFFICIENT_QUORUM` and a cooldown discriminator in the payload's `detail` field.
- Confirming-subject grants from the cooled-down subject for _other_ requests are also rejected during the cooldown window (the cooldown applies to participation, not just origination).

Cooldown is constitutional and cannot be loosened by policy bundle. The 5-minute floor is the minimum; deployments may extend it.

**Per-subject monthly cap (`OverrideRateClass`).** Every HUMAN_USER subject carries an `OverrideRateClass` value:

| Class               | Cap          | Default |
| ------------------- | ------------ | ------- |
| `LIMITED_LOW`       | ≤ 3 / month  | Yes     |
| `LIMITED_MED`       | ≤ 10 / month | No      |
| `RECOVERY_OPERATOR` | ≤ 30 / month | No      |

Default is `LIMITED_LOW`. Promotion to `LIMITED_MED` or `RECOVERY_OPERATOR` is itself a system mutation per S2.3 §26.2.2 (`RecoveryRequiredForSystemMutation`); promotion attempts outside recovery mode are hard-denied. The cap counts every successful override the subject participated in (origination + confirmation) within a rolling 30-day window. Reaching the cap denies further override requests and confirmations until the window slides.

**Rate-limit denials are evidence.** Every rate-limit denial is an `OVERRIDE_DENIED` record at FOREVER retention. The cap is therefore not silent; reaching it is itself a visible operational signal.

The reason the cap is per-subject monthly rather than per-host or per-cluster is that override authority belongs to humans, not machines. A subject whose monthly cap is exhausted cannot bypass the limit by switching hosts; a host whose hosted operators all share the same human identity does not multiply the cap. A team whose collective override budget is too small for actual operational needs is a sign that the team needs `LIMITED_MED` promotion (a recovery-mode mutation) — not that the cap should be quietly raised.

Cooldown also bounds the burn rate. Even a `RECOVERY_OPERATOR` with a 30-per-month cap cannot use all 30 in a single hour: the 5-minute cooldown caps maximum sustained rate at 12 per hour. Combined with the cap, the effective worst case is 30 in 2.5 hours, which is itself a strong signal that something incident-class is in progress and warrants operator alert.

## §12 Trust surface

Override prompts render through the trust path described by L7. Three rules:

1. **Distinct prompt node kind (queued).** Emergency override prompts SHOULD render via a dedicated `OVERRIDE_PROMPT` NodeKind. This NodeKind is **queued as a candidate addition to the L7.2 NodeKind enum** and is **not authored by this spec**. Until L7.2 is amended, override prompts fall back to the existing `APPROVAL_PROMPT` NodeKind with an `is_override = TRUE` flag at the node level. The flag is signer-controlled (per S7.2 §I3 trust-bearing rule); subjects cannot author it themselves.
2. **Visually distinct.** Override prompts MUST be visually distinct from approval prompts. The visual distinction is enforced via L7.3 visual language; the rule cited is the L7.3 ΔE ≥ 25 distinctness rule for the HARMFUL state, applied to the override prompt's chrome. A theme that fails to satisfy ΔE ≥ 25 is rejected as a `THEME_INVARIANT_VIOLATED` per S7.3.
3. **Chrome-zone authored.** Override prompts render in the CHROME composition zone (S7.1) and survive fullscreen, kiosk mode, and recovery-mode hardening. The CHROME zone is exclusively `AIOS_SURFACE` per INV-020. App surfaces cannot fake an override prompt.

Cross-spec handoffs (all abstract, none authored here):

- L7.1 surface composition rules: CHROME zone authority, fullscreen survival.
- L7.2 NodeKind enum: queued `OVERRIDE_PROMPT` addition.
- L7.3 visual language: ΔE ≥ 25 distinctness for HARMFUL state, motion duration `MOTION_DURATION_INSTANT = 0 ms` for the security indicator portion, multi-axis distinction for the override-vs-approval pair.

The override prompt MUST display, at minimum: the requesting subject's canonical id, the target action's canonical hash (truncated for display per S0.1 §8.5 conventions), the target hard-deny rule id, the strength tier, the TTL ceiling, and the justification text (post-redaction). Any prompt that omits these fields fails L7.2 schema validation.

Authorship of override prompts. Per S7.2 §I3 ("trust-bearing kinds constitutionally cannot be authored by AI subjects"), an `APPROVAL_PROMPT` (and the queued `OVERRIDE_PROMPT`) cannot be authored by an AI subject. The Override Manager itself authors the prompt's UI tree, signing it with its own service identity. AI subjects in the loop (for instance an agent that surfaced the operational question that led to the override) cannot inject a prompt into the chrome zone.

Recovery-mode trust surface. In recovery mode the override prompt inherits the recovery theme (per INV-022, recovery aesthetically distinct). The visual distinction between a recovery-mode override prompt and a normal-mode override prompt is itself a defence: an operator who sees a recovery-themed override prompt while believing they are in normal mode (or vice versa) is alerted to a potential session-mode confusion before signing.

## §13 Evidence (FOREVER)

Every override-relevant event produces an evidence record. All record types in this section MUST be assigned `RetentionPolicy = FOREVER` in S3.1's retention table. The constitutional consequence is that **evidence storage MUST guarantee the FOREVER tier never garbage-collects** the override record set; compaction (S3.1 §12) MAY tombstone payloads but the receipt identities and chain linkage MUST be preserved indefinitely.

```proto
enum OverrideRecordType {
  OVERRIDE_RECORD_TYPE_UNSPECIFIED = 0;
  OVERRIDE_REQUESTED         = 1;
  OVERRIDE_QUORUM_RECEIVED   = 2;
  OVERRIDE_GRANTED           = 3;
  OVERRIDE_CONSUMED          = 4;
  OVERRIDE_DENIED            = 5;
  OVERRIDE_EXPIRED           = 6;
  OVERRIDE_REVOKED           = 7;
  OVERRIDE_REVIEW            = 8;
}
```

Each enum value corresponds to one new `RecordType` queued for addition to the closed S3.1 `RecordType` vocabulary (§24.1 currently terminates at 87 entries; the eight additions here advance the count by 8 once consolidated). The mapping is:

| OverrideRecordType         | When emitted                                                                             |
| -------------------------- | ---------------------------------------------------------------------------------------- |
| `OVERRIDE_REQUESTED`       | An OverrideRequest entered `OS_REQUESTED`                                                |
| `OVERRIDE_QUORUM_RECEIVED` | A confirming signature arrived but quorum not yet met (`OS_AWAITING_DUAL_CONFIRM` step)  |
| `OVERRIDE_GRANTED`         | The FSM transitioned to `OS_ACTIVE` and a binding was issued                             |
| `OVERRIDE_CONSUMED`        | The Capability Runtime executed the bound action under the override                      |
| `OVERRIDE_DENIED`          | Any of the §3.5 `OverrideDenialReason` codes fired                                       |
| `OVERRIDE_EXPIRED`         | TTL elapsed without consumption                                                          |
| `OVERRIDE_REVOKED`         | An ACTIVE binding was revoked before consumption                                         |
| `OVERRIDE_REVIEW`          | A post-hoc forensic review or attestation referencing one or more prior override records |

`OVERRIDE_REVIEW` is the **only** way the override record set is augmented after the fact. Reviews are new records that reference older records by `override_id` or `override_request_id`; they do not modify and cannot replace earlier records. INV-005 (evidence append-only) is preserved.

Operational consequences:

- Storage must guarantee FOREVER retention for the eight record types. S3.1 §13 lists FOREVER-retention record types; the eight additions sit alongside `RECOVERY_EVENT`, `TAMPER_DETECTED`, `EMERGENCY_OVERRIDE_GRANT` (note the rev.1-era name in S3.1 §4 — this spec's `OVERRIDE_GRANTED` is its rev.2 successor; consolidation will reconcile naming).
- Cold-tier movement (S3.1 §7.4) is permitted but the constitutional guarantee is that retrieval remains possible. `OVERRIDE_GRANTED`, `OVERRIDE_CONSUMED`, `OVERRIDE_DENIED` SHOULD remain in hot tier alongside `RECOVERY_EVENT` and `TAMPER_DETECTED` for incident-response readiness.
- Compaction summaries (S3.1 §12) MAY roll up `OVERRIDE_REVIEW` records into hourly summaries for very high-volume forensic projects, but receipt identities are preserved as tombstones.

Each record's payload includes the relevant ids (`override_request_id`, `override_id`, `requesting_subject_id`, `confirming_subject_ids`, `target_action_request_id`, `target_action_canonical_hash`, `target_hard_deny_rule_id`), the strength/scope/ttl class, and the redacted `justification_text`. Secret-shaped redaction follows S3.1 §14 default profile.

Append-authority discipline (per S3.1 §17). `OVERRIDE_REQUESTED`, `OVERRIDE_QUORUM_RECEIVED`, `OVERRIDE_GRANTED`, `OVERRIDE_CONSUMED`, `OVERRIDE_DENIED`, `OVERRIDE_EXPIRED`, `OVERRIDE_REVOKED`, and `OVERRIDE_REVIEW` records may be emitted only by the Override Manager service or by the Capability Runtime when reporting consumption. Emission attempts from any other subject — most importantly any AI subject — are hard-denied at the evidence log surface and themselves emit `TAMPER_DETECTED` per S3.1 §11.5. INV-005 (evidence append-only) and INV-016 (AI cannot grade its own work) hold.

Telemetry bounds. The override telemetry adds the following counters with bounded label cardinality (per S3.1 §20 conventions; subject ids never appear as labels):

| Metric                                  | Type    | Labels (closed)                                                  |
| --------------------------------------- | ------- | ---------------------------------------------------------------- |
| `override_requests_total`               | counter | `strength` (3 values), `scope` (2 values)                        |
| `override_grants_total`                 | counter | `strength`, `ttl_class` (3 values)                               |
| `override_consumed_total`               | counter | `strength`, `scope`                                              |
| `override_denied_total`                 | counter | `denial_reason` (8 values from §3.5)                             |
| `override_expired_total`                | counter | `ttl_class`                                                      |
| `override_revoked_total`                | counter | `reason` (closed: `OPERATOR`, `SYSTEM`, `CLOCK_REWIND`)          |
| `override_review_total`                 | counter | `referenced_record_type` (closed: 7 values from §3 minus REVIEW) |
| `override_active_bindings`              | gauge   | `strength`                                                       |
| `override_cooldown_subjects_active`     | gauge   | none                                                             |
| `override_rate_class_utilisation_ratio` | gauge   | `rate_class` (3 values)                                          |

Cardinality budget: ≤ 60 active label tuples across the full set. Subject, group, channel ids are never labels — they would inflate cardinality unboundedly and would re-introduce subject identity into the metrics surface that S3.1 §20 forbids.

## §14 Recovery-mode special case

Recovery boot is the only context in which `STRONG_SOLO` is permitted. The rules are intentionally narrow:

1. **Recovery boot must be active.** `Session.recovery_mode = true` at the moment of grant (S5.1 §7). The Override Manager re-checks `recovery_mode` at grant time, not just at request time, to defend against session demotion.
2. **The requesting subject must be `_system` scope.** Per S5.1 §7.1 the only legitimate recovery subjects are `_system:remote:operator-<id>`, `_system:local:operator-<id>`, and `_system:service:recovery-diagnostics`. Of these, only the human operator forms (`remote:` and `local:`) may originate or confirm overrides; service kinds are excluded.
3. **`NonOverridableClass` is still non-overridable.** Recovery mode grants `STRONG_SOLO` strength; it does not unlock the constitutional classes. An attempt to override `EVIDENCE_LOG_REWRITE` from recovery is denied with `TARGET_NOT_OVERRIDABLE` and emits `OVERRIDE_DENIED` + `OVERRIDE_REVIEW`.
4. **TTL ceiling is `TTL_OVERRIDE_RECOVERY` (≤ 15 min).** Recovery does **not** unlock longer TTLs.
5. **Recovery cooldown is shared with normal mode.** The 5-minute cooldown applies regardless of mode; reboot-into-recovery does not reset the cooldown timer.
6. **Recovery rate cap applies.** Recovery operators carry `OverrideRateClass = RECOVERY_OPERATOR` (≤ 30/month). Burning through the cap denies further overrides until the window slides.

Outside recovery, `STRONG_SOLO` is constitutionally rejected. The minimum strength outside recovery is `DUAL_HUMAN`. The Override Manager does not negotiate this floor.

The recovery-mode special case binds INV-001 (recovery independent of L5), INV-004 (recovery boundary), the conjunction of INV-002 + INV-005 + INV-014 (recovery overrides are HUMAN_USER-originated, FOREVER-signed, and have no validity without FOREVER evidence — so recovery is loud, signed, and never silent), and INV-022 (recovery aesthetic distinct — the override prompt's chrome inherits the recovery theme so the operator cannot mistake the moment for normal mode).

Why `STRONG_SOLO` is acceptable in recovery and only in recovery. The threat model that demands `DUAL_HUMAN` outside recovery is "one running AIOS, one compromised session". In recovery boot the system is **not** running normally: AIOS-FS is mounted read-only or in a recovery profile, the Cognitive Core is stopped, the network stack is gated, and the only active subject is the recovery operator at the recovery console. The compromise surface is therefore narrower by construction. The operator is also physically present (in the `_system:local:` case) or operating through a signed recovery credential (in the `_system:remote:` case). `STRONG_SOLO` accepts the trade: one human operator with strong reauth, in an environment where the rest of AIOS is intentionally inert.

Recovery session expiry interacts with override TTL. Per S5.1 §7.3, recovery sessions have a hard 8-hour ceiling. An `OS_ACTIVE` binding whose `expires_at` is within the recovery session's `expires_at` remains valid until consumption, expiry, or revocation. If the recovery session itself expires before the binding consumes, the binding is automatically revoked with `OS_REVOKED` and an `OVERRIDE_REVOKED` record is emitted citing `reason = SYSTEM` and a payload note describing the recovery-session expiry.

Defence against recovery-mode mis-entry. INV-022 (recovery aesthetic distinct) ensures the operator can see they are in recovery; this spec's §12 visual rules ensure the override prompt is itself visually distinct. The combination means an operator cannot mistakenly grant a normal-mode override while in recovery (the prompt looks recovery-themed) nor a recovery-mode override while in normal mode (the prompt looks normal-themed and `STRONG_SOLO` is not on offer). The two visual axes — recovery-vs-normal and override-vs-approval — together produce four distinguishable prompts.

## §15 Reversibility and review

An override that has been `OS_CONSUMED` cannot be unwound by another override. The reasoning is mechanical: the bound action's effects are physical-world (a deleted backup is gone, a modified boot record is modified). The FSM honors this with `OS_CONSUMED` as a terminal state.

Three properties hold:

1. **Future overrides change behavior going forward, not backward.** A later override may revoke a still-`OS_ACTIVE` binding (`OS_REVOKED`). It may also issue a new override that compensates (e.g., re-create a deleted resource from a different source). It cannot delete the prior `OVERRIDE_CONSUMED` record.
2. **The override record set is immutable (FOREVER retention).** Per §13 every override record is FOREVER-retained. No mechanism exists to modify, delete, or reorder them. INV-005 holds.
3. **Post-hoc forensic claims are recorded as `OVERRIDE_REVIEW`, never as modification.** If an operator later concludes that a prior override was misuse, the conclusion is a new `OVERRIDE_REVIEW` record referencing the prior `override_id`. The original `OVERRIDE_GRANTED` and `OVERRIDE_CONSUMED` records are not touched.

This is the L4 mirror of S3.1 §2.3 ("Corrections are new evidence records that reference older records. Original records are never rewritten") applied to the override domain.

A practical corollary. If a future audit determines that an `OS_CONSUMED` override was misuse, the audit's conclusion is recorded as `OVERRIDE_REVIEW`. The misuse is now documented in the same chain as the original override; both are FOREVER-retained; the chain hash links them. A subsequent auditor can replay the chain and arrive at the same conclusion. There is no mechanism by which the misuse can be hidden or the original override unmade.

This property means override misuse is bounded but not unbounded. An attacker who somehow obtains both `DUAL_HUMAN` confirmations and successfully consumes a malicious override is constrained: the action's blast radius is bounded by `ONE_ACTION` or `ONE_SUBJECT_TTL` scope; the act is FOREVER-recorded; the requesting and confirming subjects are named in the record; the rate cap was decremented; the cooldown applies; and any subsequent `OVERRIDE_REVIEW` will carry the misuse forward as part of the visible trail. The cost of misuse is therefore high and persistent — which is exactly the cost the constitution intends.

## §16 Cross-references

All cross-references in this spec are **abstract** unless the target spec is a peer L4 file. The Override Manager consumes ApprovalChannel from S5.3 without redefining the enum; consumes `ON_REVEAL_ONLY` flagging from S5.2 without redefining the vault contract; consumes `RetentionPolicy = FOREVER` from S3.1 §13 without redefining the retention semantics. Where a target spec is queued for addition (e.g. L7.2 `OVERRIDE_PROMPT`), this spec calls out the queue explicitly and falls back to the existing contract.

| Spec | Direction           | What this spec consumes / produces                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| ---- | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| S2.3 | consumer            | Hard-deny rule ids and `NonOverridableClass` mapping; `request_hash` canonical-hash convention                                                                                                                                                                                                                                                                                                                                                                                                                     |
| S5.1 | consumer            | `Subject`, `SubjectKind = HUMAN_USER`, `Session`, `recovery_mode`, `_system` scope, `idbundle_version`                                                                                                                                                                                                                                                                                                                                                                                                             |
| S5.2 | consumer (abstract) | Vault `ON_REVEAL_ONLY` flagging that defines `VAULT_RAW_REVEAL_BYPASS` non-overridability                                                                                                                                                                                                                                                                                                                                                                                                                          |
| S5.3 | consumer (abstract) | `ApprovalChannel` enum used for channel-separation discipline; the contract belongs to S5.3                                                                                                                                                                                                                                                                                                                                                                                                                        |
| S0.1 | consumer            | `request_hash` canonical hash convention `hex_lower(BLAKE3(JCS(...)))[:32]`                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| S3.1 | producer            | Eight new `RecordType` entries queued (`OVERRIDE_REQUESTED`, `OVERRIDE_QUORUM_RECEIVED`, `OVERRIDE_GRANTED`, `OVERRIDE_CONSUMED`, `OVERRIDE_DENIED`, `OVERRIDE_EXPIRED`, `OVERRIDE_REVOKED`, `OVERRIDE_REVIEW`); all FOREVER retention                                                                                                                                                                                                                                                                             |
| L1   | consumer            | Recovery boundary — `STRONG_SOLO` permitted only in recovery boot                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| L7.1 | consumer            | CHROME composition zone for override prompts                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| L7.2 | consumer            | NodeKind enum; queued addition `OVERRIDE_PROMPT`; fallback `APPROVAL_PROMPT` with `is_override = TRUE`                                                                                                                                                                                                                                                                                                                                                                                                             |
| L7.3 | consumer            | Visual language ΔE ≥ 25 distinctness rule for HARMFUL state                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| L0   | consumer            | INV-001 (recovery independent of L5), INV-002 (AI proposes never executes — AI subjects cannot originate or confirm an override), INV-003 (secrets are capabilities), INV-004 (recovery boundary), INV-005 (evidence append-only), INV-008 (default deny), INV-014 (no proof, no completion — an override claim with no FOREVER evidence has no validity), INV-018 (vault never leaks raw secrets), INV-019 (visual identity preserved), INV-020 (trust indicators visible), INV-022 (recovery aesthetic distinct) |

Each consumer relationship is **read-only** from this spec's perspective. This spec does not redefine, extend, or weaken any contract owned by another spec. The Override Manager is a downstream client of S2.3 (it asks "is this rule overridable?"), of S5.1 (it asks "is this subject HUMAN_USER?"), of S5.3 (it asks "is this ApprovalChannel value distinct from that one?"), and of S3.1 (it appends evidence records). Each cross-spec call is structurally bounded by the consumed contract; this spec adds no new responsibilities to those upstream contracts beyond what the queued `OVERRIDE_PROMPT` and the eight new RecordType values represent.

Producer relationships (one row of the table) are limited to S3.1: this spec produces eight new `RecordType` values, all FOREVER-retained, all governed by the existing S3.1 append-authority and chain-integrity discipline. The values are queued; the consolidated S3.1 vocabulary advances from 87 to 95 entries once integrated. No other spec is amended by this file.

### §16.1 Adversarial robustness summary

For implementation reviewers, this section summarises the adversarial postures the Override Manager must defend against and the structural defences enumerated in this spec. Each posture cites the section that owns the defence.

| Adversarial posture                                 | Structural defence                                                                  | Section  |
| --------------------------------------------------- | ----------------------------------------------------------------------------------- | -------- |
| Forged request signature                            | Ed25519 verify on `requesting_subject_signature` against active identity bundle     | §4       |
| Forged binding signature                            | Ed25519 verify on `issuer_signature` against Override Manager's signing key         | §5       |
| Subject claims HUMAN_USER without being one         | Identity service enforces `kind` at registration; signed in `NormalizedSubject`     | S5.1 §10 |
| Single-channel "two grants" (compromised laptop)    | ApprovalChannel distinctness rule                                                   | §7       |
| Same subject signs twice across channels            | Subject distinctness rule; precedes channel distinctness                            | §7       |
| AI subject originates or confirms                   | Constitutional kind check; AI subjects are denied `SUBJECT_NOT_HUMAN`               | §3.5     |
| Action mutated between policy decision and override | `target_action_canonical_hash` binds to exact JCS canonical hash                    | §4       |
| `NonOverridableClass` target attempted              | Immediate `TARGET_NOT_OVERRIDABLE`; recorded; `OVERRIDE_REVIEW` triggered           | §10      |
| TTL extended beyond ceiling                         | Ceilings constitutionally fixed; computed at grant; no extension API                | §9       |
| Cooldown bypass via reboot or new session           | Cooldown is per-subject across sessions and across modes                            | §11      |
| Rate-cap bypass via different host                  | Cap is per-subject; canonical subject id identifies one operator regardless of host | §11      |
| Bundle-loosening of minimum strength                | `InvariantLooseningAttempted` rejection at S2.3 bundle compile                      | §8       |
| Override-of-override race                           | Forbidden; revocation only via operator action recorded as `OVERRIDE_REVOKED`       | §6       |
| Replay of stale request id                          | ULID monotonicity + idempotency window; duplicate returns existing id               | §6       |
| Clock-rewind grants extra TTL                       | Override Manager rewinds → all `OS_ACTIVE` revoked + `RECOVERY_EVENT`               | §9, §6   |
| Evidence-emit forgery from non-Override-Manager     | Append authority on RecordType; foreign emit produces `TAMPER_DETECTED`             | §13      |
| Phishing prompt mimicking override prompt           | CHROME zone exclusivity; theme distinctness ΔE ≥ 25; signed UI tree                 | §12      |
| Recovery-mode override of constitutional class      | `NonOverridableClass` is non-overridable in recovery too                            | §10, §14 |

The cumulative property is "no single failure unlocks override." A successful override requires (a) at least one HUMAN_USER subject, (b) for non-recovery, at least two distinct subjects on two distinct channels, (c) a signed identity bundle that binds the subjects to their kind, (d) a valid policy bundle that does not loosen the minimum strength, (e) an Override Manager whose signing key is intact, (f) an evidence log whose chain is intact, (g) a clock that has not rewound, (h) cooldown and cap headroom for every participating subject. Every one of those preconditions is independently audited.

## §17 Worked examples

### Example A — Recovery operator unbricks the policy DB

**Setup.** A bad policy bundle deploy left the Policy Kernel in degraded mode. The recovery operator must hard-reset the policy DB to a known-good snapshot. The action is hard-denied under normal mode (`hd.policy_log_mutation` per S2.3 §6) but the rule is **not** in `NonOverridableClass`.

**Flow.**

1. Operator boots into the L1 recovery-safe kernel.
2. Operator authenticates via signed operator credential. `Session.recovery_mode = true`, `subject_canonical_id = _system:local:operator-247`, `session_class = RECOVERY`.
3. Operator authors `OverrideRequest` with `strength = STRONG_SOLO`, `scope = ONE_ACTION`, `ttl_class = TTL_OVERRIDE_RECOVERY`, `target_hard_deny_rule_id = "hd.policy_log_mutation"`, `justification_text = "Restoring policy DB to good snapshot 0x7f...; policy bundle 0xab... pushed bad rule cycle, kernel degraded since 2026-05-09T12:14Z"` (148 chars).
4. The Override Manager verifies recovery mode is active, the rule is overridable, and the operator's `OverrideRateClass = RECOVERY_OPERATOR` cap is not exhausted.
5. Single confirmation accepted because of `STRONG_SOLO` + recovery. FSM transitions `OS_REQUESTED → OS_ACTIVE`.
6. `OVERRIDE_REQUESTED`, `OVERRIDE_GRANTED` records emitted at FOREVER retention.
7. Capability Runtime executes the bound `policy.kernel.restore` action under the override.
8. `OVERRIDE_CONSUMED` record emitted at FOREVER retention.
9. Operator's per-subject cooldown begins; further override origination from this subject blocked for 5 minutes.

**Evidence chain produced.** The chain head moves through `OVERRIDE_REQUESTED` → `OVERRIDE_GRANTED` → `EXECUTION_STARTED` (Capability Runtime, S3.1 §4) → `EXECUTION_COMPLETED` (S3.1 §4) → `OVERRIDE_CONSUMED`. Five FOREVER-retained or otherwise long-retained records link the operator's intent to the executed action. A subsequent `VerifyChain` per S3.1 §5.3 traverses the linkage end-to-end and re-derives the operator's signature from the bundled subject identity bundle.

**What this example does not unlock.** Even at recovery + `STRONG_SOLO` + 15-minute TTL, the operator cannot:

- Override an `EVIDENCE_LOG_REWRITE` target (constitutional class).
- Forge a Subject (`IDENTITY_KEY_FORGE` is non-overridable).
- Replace the AIOS chrome (`AIOS_CHROME_REPLACEMENT` is non-overridable).
- Extend the TTL beyond 15 minutes.
- Skip the cooldown by rebooting back into recovery.
- Avoid the rate cap by switching to a different `_system:local:operator-<id>` (the cap is per-subject, and the recovery operator credential identifies one canonical subject regardless of console).

### Example B — Two human admins delete an old encrypted backup

**Setup.** Retention policy hard-denies deletion of an encrypted backup that has not yet aged past the retention horizon. The two household admins have explicitly decided the backup must go (legal hold lifted, storage exhausted).

**Flow.**

1. Admin A authors `OverrideRequest` with `strength = DUAL_HUMAN`, `scope = ONE_ACTION`, `ttl_class = TTL_OVERRIDE_SHORT`, `target_hard_deny_rule_id = "hd.retention_violation"` (illustrative — the actual rule id lives in S2.3's catalog), `justification_text` describing the legal-hold lift and storage motive.
2. Admin A signs on ApprovalChannel `KDE_LOCAL` from Session_A.
3. `OVERRIDE_REQUESTED` and `OVERRIDE_QUORUM_RECEIVED` records emitted; FSM enters `OS_AWAITING_DUAL_CONFIRM`.
4. Admin B receives the prompt on ApprovalChannel `WEB_LOCAL` from Session_B (different channel, different session, different subject). Admin B signs.
5. Override Manager verifies channel-separation: `KDE_LOCAL ≠ WEB_LOCAL`, `Session_A ≠ Session_B`, `subject_A ≠ subject_B`. Quorum satisfied.
6. FSM transitions to `OS_ACTIVE`. Binding issued with `expires_at = granted_at + 5 min`. `OVERRIDE_GRANTED` record emitted.
7. Within the 5-minute window, the Capability Runtime executes the deletion under the override.
8. `OVERRIDE_CONSUMED` record emitted.
9. Both admins enter the per-subject cooldown.

**What channel separation defended against.** Suppose Admin A's KDE session was compromised and a piece of malware on the laptop attempted to also produce a "grant" on the same session pretending to be Admin B. The Override Manager would reject the second grant with `CO_SUBJECT_IDENTICAL` because Session_A is a single session — Admin B's grant must originate in a different session, and that session must have completed S5.1 authentication independently. The malware would have to also compromise Admin B's separate device to forge the second signature. The threshold is "compromise two physical devices belonging to two different humans", which is materially harder than "compromise one device".

**Rate cap interaction.** Both admins started the month at `LIMITED_LOW = 3`. After this override, both have 2 remaining. If the household needs to repeat the operation later in the month, they will run out of cap before the rolling 30-day window slides. A household whose normal operations require more frequent overrides should request `LIMITED_MED` promotion via a recovery-mode mutation — which is itself a `RecoveryRequiredForSystemMutation` per S2.3 §26.2.2. The system structurally pushes back against routine override use.

### Example C — Attempted override of `EVIDENCE_LOG_REWRITE` is denied

**Setup.** A misguided operator tries to override the evidence log mutation hard-deny in order to "clean up" a prior incident's audit trail.

**Flow.**

1. Operator authors `OverrideRequest` with `target_hard_deny_rule_id = "hd.evidence_log_mutation"`, `strength = TRIPLE_HUMAN` (the operator believes maximum quorum will satisfy the constitution).
2. Override Manager looks up `hd.evidence_log_mutation` in the policy bundle, finds it maps to `NonOverridableClass.EVIDENCE_LOG_REWRITE`.
3. Request is **immediately** denied with `TARGET_NOT_OVERRIDABLE`. No quorum is collected.
4. `OVERRIDE_DENIED` record emitted at FOREVER retention with reason `TARGET_NOT_OVERRIDABLE` and the full request payload (post-redaction) preserved.
5. `OVERRIDE_REVIEW` record automatically emitted, referencing the `OVERRIDE_DENIED` record. The very attempt is now part of the audit witness.
6. The operator's monthly rate counter advances — a denied attempt counts.
7. Tamper-detection scheduled audit notes the attempt; operator alert raised. INV-005 (evidence append-only) and the conjunction INV-002 + INV-014 (no AI-originated override; an override claim without FOREVER evidence has no validity) hold.

**Why even the attempt is recorded.** A common attacker pattern is "probe before action": an attacker first tries the smallest possible override to learn what the system will accept, then escalates. By making the very attempt FOREVER-evidenced, this spec converts probing into a high-cost activity. The attacker's probe is now permanent in the audit trail, named to the requesting subject, and visible to any operator running an `OVERRIDE_REVIEW` query.

**Forensic value of denied overrides.** In a post-incident review, the population of `OVERRIDE_DENIED` records is itself diagnostic. Patterns of attempted non-overridable actions, attempted broad scopes, or attempted single-channel grants are signals about the operator's intent and competence. A subject with many `SCOPE_TOO_BROAD` denials is either being trained, being attacked, or attempting to fish for a constitutional weakness — all three deserve human follow-up. A subject with many `TARGET_NOT_OVERRIDABLE` denials against `EVIDENCE_LOG_REWRITE` is, on its face, attempting to evade audit and warrants immediate operator attention.

These three examples cover the three operating modes: recovery-mode `STRONG_SOLO`, normal-mode `DUAL_HUMAN`, and refusal of a `NonOverridableClass` target. The constitution's behavior in each is fully specified. They are intentionally minimal; they exercise the request shape, the FSM, the quorum check, the rate-cap interaction, the channel-separation defence, and the constitutional refusal path. A full acceptance harness for S5.4 (deferred to implementation phase) will produce dozens of additional fixtures covering edge cases such as confirm-window expiry, mid-flight subject revocation, clock-rewind handling, recovery-session expiry interleaved with override TTL, and FOREVER-retention compaction.

## §18 Open questions (deferred)

- **L7.2 `OVERRIDE_PROMPT` NodeKind addition.** This spec queues the addition; L7.2 owns the actual NodeKind closed-enum amendment. Until amended, override prompts ride the `APPROVAL_PROMPT` NodeKind with the `is_override = TRUE` flag. Consolidation work is tracked separately.
- **Cross-machine override federation.** When AIOS becomes multi-host, an override on one host does not automatically apply on another. The federation rules — whether `OVERRIDE_GRANTED` propagates, how channel separation is satisfied across hosts, how cooldowns combine — are deferred.
- **Override delegation.** Whether a subject may delegate their confirming signature to a hardware token or a delegated-authority subject is deferred. The current contract requires the subject's own Ed25519 signature.
- **Programmatic override APIs for incident-response runbooks.** A scripted, audited override path (still requiring human signatures on the back end) for predictable incident-response flows. Deferred to a future S5.4 revision.
- **Override-of-override semantics.** Can a `TRIPLE_HUMAN` override countermand a `DUAL_HUMAN` override mid-flight (between `OS_ACTIVE` and `OS_CONSUMED`)? The current contract says no — revocation is by operator action recorded as `OVERRIDE_REVOKED`, not by competing override. Future work may relax this.
- **Per-class TTL fine-tuning.** The current three TTL classes (`INSTANT`/`SHORT`/`RECOVERY`) cover the common cases. Whether additional classes are needed for specific incident-response patterns is deferred until evidence accumulates.
- **Operator-visible override dashboard.** A dedicated L9 view summarising active bindings, recent grants, denials, cooldowns, and rate-class utilisation. Deferred to L9 admin operations sub-spec.
- **Hardware-token confirmation channel.** Whether a hardware security key (e.g. WebAuthn/U2F device) constitutes its own ApprovalChannel value or merely satisfies the `STRONG` session class within an existing channel. Deferred to S5.3.
- **Override propagation across worktree branches.** In multi-host AIOS clusters with shared evidence, an override on host A must not silently grant on host B; the federation contract must spell this out. Deferred to a federation sub-spec.
- **Pre-authorised override templates.** Whether common incident-response patterns can be pre-authored as templates that operators fill in at incident time, reducing the cognitive load of authoring `justification_text` from scratch. The current contract requires per-incident authorship; templates would be additive. Deferred.
- **Reverse-side accountability.** Whether the absence of a confirming subject's signature within the confirm window should escalate (e.g. emit a `OVERRIDE_AWAITING_OVERDUE` evidence record) so that confirming-subject attentiveness is itself observable. Deferred.

## §19 Status & evidence grade

This spec is `REAL` at evidence grade `E1`. The "REAL" status is justified at E1 because the grade-axis requirement for this sub-spec is "file exists" — a contract-grade specification that defines the closed vocabularies, FSM, record shapes, and constitutional invariants for emergency override. Implementation evidence (E2 build/typecheck of generated proto, E3 unit tests against the FSM, E4 e2e rehearsal of recovery-mode override) is queued for downstream phases and tracked under the L4 implementation roadmap, not in this spec.

Specifically, the path from E1 to higher grades is:

- **E2 — Build/typecheck.** Compile the `aios.override.v1alpha1` proto package against the canonical proto toolchain. Verify the closed enums, the `OverrideRequest` and `OverrideBinding` messages, and the eight-value `OverrideRecordType` all generate valid bindings in Rust (Override Manager service) and TypeScript (renderer prompts). E2 is reachable as soon as the proto file is authored and CI builds clean.
- **E3 — Unit/integration tests.** Drive the FSM through every allowed transition and every forbidden transition. Assert that quorum-and-channel-separation rules deny every `CO_SUBJECT_IDENTICAL` configuration. Assert that `NonOverridableClass` targets are denied at `OS_REQUESTED` without quorum collection. Assert that TTL expiry produces `OS_EXPIRED` and that consumed bindings are terminal. The test plan also exercises the rate cap and the cooldown discipline. E3 is reachable when the Override Manager service's first implementation passes its unit suite.
- **E4 — End-to-end / recovery rehearsal.** Boot a system into recovery mode, author a `STRONG_SOLO` override, consume it, verify the FOREVER evidence trail end-to-end via `VerifyChain`, then observe that subsequent attempts to extend or replay the binding are rejected. The rehearsal is per S6.2 §10 a recovery-critical evidence requirement; without it, S5.4 cannot legitimately move beyond E3.
- **E5 — Operational.** Live operation across multiple AIOS deployments with rolling 7-of-14 day health receipts proving the Override Manager is reachable, signing keys are rotated per S5.2's vault contract, and the FOREVER-retention guarantee is being honored by the storage tier. E5 is the steady-state grade for production AIOS.

The grade ladder above is informative; the authoritative grade requirements live in S6.2 (`02_evidence_grades.md`). This sub-spec restates the path solely so reviewers can see how the contract enables each grade transition without further interpretation. In particular, S5.4's contract has been written so that an implementer can drive E2 entirely from the schemas in §3, §4, §5, §13; can drive E3 entirely from the FSM in §6 and the rules in §7, §8, §9, §10, §11; and can drive E4 entirely from the worked examples in §17 plus the recovery special case in §14.

### §19.1 Integration checklist for L4 consolidation

When this sub-spec is consolidated into the broader L4 contract bundle, the following integration points must be honored. None of them is authored in this file, but each one is necessary for S5.4 to function end-to-end:

1. **S2.3 `NonOverridableClass` mapping.** The Policy Kernel's hard-deny rules must be tagged with their `NonOverridableClass` membership (or absence of membership). The §10 enum lives here; the rule-to-class mapping lives in S2.3. The Override Manager calls into the Policy Kernel via a yet-to-be-named RPC (`IsRuleOverridable(rule_id) → bool + class`) that returns either "overridable" or the `NonOverridableClass` value.
2. **S5.3 `ApprovalChannel` enum.** The closed list of channel values is owned by S5.3. The Override Manager treats them as opaque distinguishers, but the values must be consistent across channels and must be signed as part of the Session record per S5.1 §8.1.
3. **S5.2 `ON_REVEAL_ONLY` flag.** The vault's per-secret flag that drives `VAULT_RAW_REVEAL_BYPASS`'s non-overridability lives in S5.2. The Override Manager does not inspect vault material; it queries the vault for "is this secret flagged ON_REVEAL_ONLY?" and hard-denies the request if yes.
4. **S3.1 RecordType extension.** The eight new RecordType values must be added to S3.1's closed enum, with payload schemas that mirror the §4 and §5 record shapes. Existing append-authority discipline applies.
5. **L7.2 NodeKind extension (optional).** The queued `OVERRIDE_PROMPT` NodeKind extension is desirable but not required for E1; until it lands, `APPROVAL_PROMPT` with `is_override = TRUE` is the contractual fallback.
6. **L9 dashboard wiring (optional).** A future operator-visible override dashboard would surface the §13 telemetry counters and the active-binding gauge. The dashboard itself is L9 work; this spec just guarantees the metrics exist with bounded cardinality.

The integration list is intentionally short. S5.4's design choice was to consume existing contracts rather than carve out new ones; the only true _additions_ to the surrounding L4 surface are the `OverrideRequest` / `OverrideBinding` records and the eight evidence record types.

### §19.2 Acceptance criteria

The following acceptance bullets translate the contract into checkable conditions. Each bullet maps to a specific section and to a future verification fixture (deferred to the implementation phase per the E2/E3/E4 ladder above).

- The closed `OverrideState` enum has exactly seven non-`UNSPECIFIED` values; bundle load fails on any other value.
- The closed `OverrideStrength` enum contains exactly `STRONG_SOLO`, `DUAL_HUMAN`, `TRIPLE_HUMAN`; no `WEAK`, no `OPEN_ACCESS`, no `BYPASS_QUORUM`.
- The closed `OverrideScope` enum contains exactly `ONE_ACTION` and `ONE_SUBJECT_TTL`; no `OPEN_SCOPE`, no `ALL_ACTIONS`, no `INDEFINITE`, no `BLANKET`.
- The closed `OverrideTtlClass` ceilings are 60 s / 5 min / 15 min and no implementation can issue a binding with `expires_at - granted_at` exceeding the ceiling.
- The closed `OverrideDenialReason` enum contains exactly the eight reasons enumerated in §3.5.
- The closed `NonOverridableClass` enum contains exactly the six classes enumerated in §10; an override targeting any class member is denied with `TARGET_NOT_OVERRIDABLE` without quorum collection.
- A request authored with `strength = STRONG_SOLO` and `recovery_mode = false` is denied at request time.
- A request whose `confirming_subject_ids` length does not match the strength tier is denied with `INSUFFICIENT_QUORUM`.
- A request whose two confirms arrive on the same ApprovalChannel value, the same Session, or from the same canonical subject is denied with `CO_SUBJECT_IDENTICAL`.
- A request whose `target_action_canonical_hash` does not match the bound action's recomputed canonical hash is denied at the Capability Runtime side as a stale binding.
- A binding whose `issuer_signature` fails Ed25519 verification is rejected at the Capability Runtime side; the failed verification emits `TAMPER_DETECTED`.
- A successful override produces FOREVER-retained `OVERRIDE_REQUESTED`, `OVERRIDE_GRANTED`, and `OVERRIDE_CONSUMED` records linked by hash chain to the bound action's `EXECUTION_*` records.
- A denied override produces a FOREVER-retained `OVERRIDE_DENIED` record with the `OverrideDenialReason` populated.
- A denied override targeting a `NonOverridableClass` rule additionally produces an `OVERRIDE_REVIEW` record referencing the denial.
- The per-subject 5-minute cooldown denies further requests from the same subject within the window; the denial is itself recorded as `OVERRIDE_DENIED` with discriminator `cooldown`.
- The per-subject monthly cap denies requests beyond `OverrideRateClass` ceiling; the denial is recorded as `OVERRIDE_DENIED`.
- An override record set is never deleted, modified, or reordered. Audit replays produce identical results across multiple invocations.

### §19.3 Closing remarks

Emergency Override is the constitutional fire alarm of AIOS. It is small by design: nine sections of vocabulary, one FSM, two record shapes, eight evidence record types, six non-overridable classes, three strength tiers, three TTL ceilings, three rate classes, one cooldown, one channel-separation rule. Everything else in this file is a tightening, a clarification, or a cross-reference into the surrounding constitution. The contract is small because it must be auditable in a single sitting by an operator under stress, and because every additional knob is a potential weakness.

The cost asymmetry between routine approval and emergency override is the central design choice. Approval is cheap because the Policy Kernel has already vetted the action's compatibility with the operating constitution; the human's role is consent, not constitutional reasoning. Override is expensive because the Policy Kernel has explicitly refused; the humans (plural, except in recovery) are now exercising constitutional authority that the kernel cannot. Making this expensive is what keeps it rare; making it auditable is what keeps it honest; making it FOREVER-retained is what keeps it permanent.

If a future revision finds this contract too restrictive in practice, the path forward is clear. New strength tiers can be added (versioned spec change). New TTL classes can be added (versioned spec change). New scopes can be added (versioned spec change). The `NonOverridableClass` set can be tightened — never loosened — by versioned spec change. What cannot be done is silent erosion: every change goes through the same constitutional discipline as the constitution itself.

The file you are reading is the contract. The behavior it describes is what AIOS will do. Anyone implementing the Override Manager has the closed enums, the FSM, the record shapes, the rule tables, the worked examples, and the integration checklist. There is no room for interpretation about "what should the system do" when an override is requested; there is only the room to write the code that enforces what the contract already says.

Status: `REAL`
Evidence: `E1`
