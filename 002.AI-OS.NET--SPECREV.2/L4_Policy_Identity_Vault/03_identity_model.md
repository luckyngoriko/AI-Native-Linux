# Identity Model (Rev.2)

| Field          | Value                                                                                                        |
| -------------- | ------------------------------------------------------------------------------------------------------------ |
| Status         | `CONTRACT` (initial; written 2026-05-09)                                                                     |
| Phase tag      | S5.1                                                                                                         |
| Layer          | L4 Policy, Identity, Vault                                                                                   |
| Schema package | `aios.identity.v1alpha1`                                                                                     |
| Consumes       | S4.1 namespace catalog, S0.1 action envelope (subject field), S2.3 policy kernel (subject normalization)     |
| Produces       | typed `Subject`, canonical subject ids, authentication context, membership graph, primary_group_id semantics |

## 1. Purpose

Every action in AIOS is bound to a subject; every policy decision keys on subject identity; every evidence record names the subject; every capability is granted to a subject. Identity is therefore the spine through which L4 (policy, vault), L5 (agents), L6 (apps), and L9 (audit) reach a coherent picture.

This spec defines:

1. The closed taxonomy of subject kinds (`SubjectKind`).
2. The canonical id format and the regex it must satisfy.
3. Group as a first-class identity unit, not an attribute.
4. Multi-group membership with a single `primary_group_id` per action context.
5. Recovery-mode subjects and their constitutional separation from normal-mode subjects.
6. The authentication context (`SessionClass`, `is_ai`, risk flags).
7. The `IdentityService` gRPC surface used by S2.3 §3 (subject normalization).
8. The capability binding skeleton (full vault API lives in `02_vault_broker.md`).

## 2. Core invariants

- **I1 — Subject is unforgeable.** Every accepted action carries a `Subject` record signed by the identity service. The Capability Runtime rejects envelopes whose subject signature does not verify under the active identity service public key.
- **I2 — `_system` is constitutional.** The `_system` scope subjects (system services, recovery operators) are a closed set defined by the signed identity bundle. They cannot be created at runtime by any action; they are introduced only via recovery boot or signed bundle update.
- **I3 — Group is a first-class unit.** A subject's identity is `<group_id>:<sub_id>` (or `_system:<service_name>` for system services). The group prefix is part of the canonical id, not an attribute. Stripping it yields an invalid subject id.
- **I4 — Single primary group per action.** A subject with memberships in groups A, B, C produces actions under exactly one `primary_group_id` at a time. Switching primary group requires re-authentication and is logged. There is no "act as multiple groups simultaneously".
- **I5 — Recovery mode separates a subject.** A normal-mode subject and the same human's recovery-mode subject are distinct principals from the policy kernel's perspective. Recovery-mode actions never flow through normal-mode rules; constitutional hard-denies in S2.3 (recovery-required-for-system-mutation) consume the recovery flag.
- **I6 — `is_ai` is set by the identity service, not by the subject.** A subject cannot self-declare as human. The classification is bound at registration and signed.
- **I7 — Capability bindings are scoped.** A capability granted to a subject in group A cannot be exercised when the subject re-authenticates with `primary_group_id = B`. The vault broker checks scope at issuance and at use.
- **I8 — Identity bundle is signed and versioned.** The active set of services, devices, and reserved subjects is loaded from a signed bundle (`idbundle_<hex>`); unsigned or signature-failing bundles put the identity service into degraded mode (only constitutional `_system` subjects available).

## 3. Subject taxonomy

```proto
enum SubjectKind {
  SUBJECT_KIND_UNSPECIFIED = 0;
  HUMAN_USER = 1;              // a person with credentials
  AI_AGENT = 2;                // an LLM-backed configured agent (group or personal)
  APPLICATION = 3;             // an L6 app instance running an action
  SERVICE = 4;                 // a system service (translator, planner, recovery diagnostics)
  DEVICE = 5;                  // a registered device (laptop, phone) acting on behalf of a user
  WORKFLOW = 6;                // a parameterized action sequence executing autonomously
  REMOTE_OPERATOR = 7;         // a human operating remotely under recovery or admin context
}
```

Closed enum. Adding a kind is a versioned spec change.

| Kind              | Default `is_ai` | Where it lives                                                 | Authentication mechanism                    |
| ----------------- | --------------- | -------------------------------------------------------------- | ------------------------------------------- |
| `HUMAN_USER`      | false           | `groups/<g>/users/<u>/`                                        | password + WebAuthn or hardware token       |
| `AI_AGENT`        | true            | `groups/<g>/agents/<a>/` or `groups/<g>/users/<u>/agents/<a>/` | signed agent manifest + agent runtime token |
| `APPLICATION`     | false           | `groups/<g>/apps/<app_id>/` or `system/apps/<app_id>/`         | app manifest + per-instance runtime token   |
| `SERVICE`         | false           | `system/agents/<service_name>/`                                | constitutional; loaded from identity bundle |
| `DEVICE`          | false           | metadata under `users/<u>/trust/devices/`                      | device cert + binding to a `HUMAN_USER`     |
| `WORKFLOW`        | false           | `groups/<g>/shared/workflows/<wf_id>/`                         | signed workflow manifest + parent action_id |
| `REMOTE_OPERATOR` | false           | recovery context only                                          | signed operator credential + recovery boot  |

Note: `is_ai = true` for `AI_AGENT` is constitutional. An app or workflow that internally uses LLMs is not classified as `AI_AGENT`; the LLM is a tool used inside the action, not the actor. Only an agent registered as `AI_AGENT` carries the `is_ai` flag, and only such subjects trigger S2.3's AI self-approval prevention (§17) and `AISystemAdminBlocked` (§26.2.3).

## 4. Canonical subject id

### 4.1 Format

```text
canonical_subject_id ::= <group_part> ":" <kind_segment> [":" <inner>]

group_part ::= group_id | "_system"
group_id   ::= matches S4.1 §7.1 regex   [a-z][a-z0-9_-]{0,62}

kind_segment by SubjectKind:
  HUMAN_USER       → <user_id>
  AI_AGENT         → <agent_id>                    (if group-owned)
                     <user_id> ":" <agent_id>      (if user-owned, personal agent)
  APPLICATION      → "app:" <app_id> ":" <instance_id>
  SERVICE          → "service:" <service_name>     (only when group_part = "_system")
  DEVICE           → <user_id> ":" "device:" <device_id>
  WORKFLOW         → "workflow:" <wf_id> ":" <run_id>
  REMOTE_OPERATOR  → "remote:" <operator_id>       (only when group_part = "_system")
```

Examples:

```text
family:alice                                # human user
family:family-assistant                     # group-owned AI agent
family:alice:diary-helper                   # personal AI agent under alice
homelab:app:bg.iconys.proxguard:i-01        # app instance
_system:service:translator                  # system service
family:alice:device:thinkpad-x1             # alice's device
finance:workflow:quarterly-close:run-7842   # workflow execution
_system:remote:operator-247                 # remote human operator
```

### 4.2 Regex

The complete canonical id must match:

```text
^[a-z_][a-z0-9_-]{0,62}(:[a-z0-9_-]+)+$
```

Total length cap: 256 bytes. Each segment's regex is tighter than the loose pattern above; the identity service validates against the kind-specific format at registration and rejects any deviation.

### 4.3 Uniqueness

Subject ids are unique within the active identity bundle. Two subjects with the same canonical id never coexist; this is enforced at registration.

### 4.4 Renaming and rebinding

A subject's canonical id is **immutable for the lifetime of its evidence trail**. Renaming is not a mutation — it requires retirement of the old id (with `RETIRED` lifecycle state) and creation of a new subject with a new id. Evidence records reference old ids forever; they are never rewritten.

## 5. Group as identity unit

### 5.1 Group object

```proto
message Group {
  string group_id = 1;                        // matches S4.1 §7.1 regex
  string display_name = 2;                    // human-readable, no semantics
  google.protobuf.Timestamp created_at = 3;
  string created_by = 4;                      // canonical_subject_id of creator
  GroupTier tier = 5;                          // closed enum
  repeated string admins = 6;                  // canonical_subject_ids
  bool can_have_ai_agents = 7;                // false locks group to humans only
  bool can_install_apps = 8;
  bool federation_eligible = 9;                // false in Rev.2 (federation deferred)
  string identity_bundle_version = 10;         // idbundle_<hex>
}

enum GroupTier {
  GROUP_TIER_UNSPECIFIED = 0;
  PERSONAL = 1;             // single household; default sandbox floor; no group_admin separation
  TEAM = 2;                 // small team; introduces group_admin role; audit logs visible to admins
  ORGANIZATIONAL = 3;       // org-level; stricter sandbox floor; mandatory approval flows
  FINANCE = 4;              // example tier with stricter floor; named by convention only
}
```

The `tier` is informative for sandbox composition (S3.2 §18) — different tiers can carry different `group_floor` profiles. The `tier` is not constitutionally enforced; it is a label that maps to a chosen group floor bundle.

### 5.2 Group registration

A group is created by:

1. A human subject in `_system` scope under recovery mode (the only way to create a group at all in Rev.2 — group registration is a system mutation).
2. A signed `GroupRegistration` action whose envelope payload contains the new group's manifest.
3. The identity service writes the group to the identity bundle, increments `idbundle_version`, and emits `GROUP_REGISTERED` evidence (FOREVER retention).

Group deletion is not in Rev.2. Groups are retired (`GroupTier` is augmented with retirement metadata) but never deleted; their evidence trail remains queryable via the privacy ceiling rules in S3.1 §23.

### 5.3 No nested groups

Per S4.1 §6 default, groups are flat in Rev.2. Subgroups are deferred. Tag-based labels via S2.1 query views can simulate hierarchy (`tags = ["finance", "audit"]`).

## 6. Membership model

### 6.1 Membership object

```proto
message Membership {
  string subject_canonical_id = 1;
  string group_id = 2;
  google.protobuf.Timestamp joined_at = 3;
  string joined_via = 4;                       // canonical_subject_id of granter
  repeated string roles = 5;                   // closed; defined per group
  bool is_primary = 6;                          // exactly one membership per subject is primary at any moment
}
```

A subject's `Memberships` is the set of all groups they belong to. Roles are group-defined and registered at group creation; the identity service does not interpret role names beyond delivering them to the policy kernel for evaluation.

### 6.2 Primary group selection

At authentication time, the subject specifies which membership becomes `primary_group_id` for the resulting session. Defaults:

- **First-time authentication:** the subject's "home" membership (set at registration).
- **Subsequent authentication:** the last `primary_group_id` used, unless the subject explicitly switches.

### 6.3 Switching primary group mid-session

`SwitchPrimaryGroup` (RPC in §13) requires re-authentication and emits `PRIMARY_GROUP_SWITCHED` evidence (STANDARD_24M retention). The current session is closed; a new session opens with the new `primary_group_id`. There is no "soft switch" that preserves session state across groups — this is a constitutional separation per I4.

### 6.4 Cross-group capability bindings

A capability granted to subject `family:alice` is bound to her `family` membership. When she re-authenticates as `homelab:alice`, that capability is **not** active in the new session. The vault broker enforces this at use time (cf. S6.1 vault broker spec, deferred).

## 7. Recovery mode

### 7.1 Recovery subject form

A recovery-mode subject's canonical id is always under `_system` scope:

```text
_system:remote:operator-<id>           # remote operator
_system:local:operator-<id>            # local operator at the recovery console
_system:service:recovery-diagnostics    # the recovery service itself
```

There is no path for a normal-mode `family:alice` subject to enter recovery mode by flag-flipping. Recovery is entered by:

1. Booting into the recovery-safe kernel (L1 recovery path — out of scope for this spec).
2. Authenticating via signed operator credential.
3. The identity service issuing a recovery-mode session whose subject id is `_system:...`.

### 7.2 Recovery flag is not loosenable

A normal-mode session cannot transition to a recovery-mode session by capability grant or policy override. The only path is reboot into recovery. This is constitutional per I5.

### 7.3 Recovery session expiry

Recovery sessions have a hard maximum duration of **8 hours**. After expiry, the session is closed and recovery mode is exited via reboot. There is no extension mechanism in Rev.2.

## 8. Authentication context

### 8.1 Session

```proto
message Session {
  string session_id = 1;                       // sess_<ulid>
  string subject_canonical_id = 2;
  string primary_group_id = 3;
  SessionClass session_class = 4;
  bool is_ai = 5;                               // mirrors subject's is_ai; included for cheap policy checks
  bool recovery_mode = 6;
  google.protobuf.Timestamp authenticated_at = 7;
  google.protobuf.Timestamp expires_at = 8;
  repeated RiskFlag risk_flags = 9;
  string identity_bundle_version = 10;
  bytes ed25519_signature = 11;                // identity service signs (subject_canonical_id || session_id || expires_at || ...)
}

enum SessionClass {
  SESSION_CLASS_UNSPECIFIED = 0;
  PUBLIC = 1;             // very weak; e.g., voice assistant before authentication
  INTERACTIVE = 2;         // logged in via UI on a known device
  STRONG = 3;              // re-authenticated within last N minutes; WebAuthn or hardware token
  RECOVERY = 4;            // recovery-mode session
  SERVICE = 5;             // system service; non-interactive
}

message RiskFlag {
  RiskFlagKind kind = 1;
  string detail = 2;
}

enum RiskFlagKind {
  RISK_FLAG_KIND_UNSPECIFIED = 0;
  AUTH_FROM_NEW_DEVICE = 1;
  AUTH_FROM_NEW_LOCATION = 2;
  RECENT_FAILED_ATTEMPTS = 3;
  PASSWORD_RECENTLY_CHANGED = 4;
  SUBJECT_RECENTLY_CREATED = 5;
  OFF_HOURS = 6;
  ELEVATED_PRIVILEGE_REQUEST = 7;
}
```

### 8.2 Session ttl

| Session class | Default ttl | Hard maximum |
| ------------- | ----------- | ------------ |
| `PUBLIC`      | 5 minutes   | 15 minutes   |
| `INTERACTIVE` | 8 hours     | 24 hours     |
| `STRONG`      | 30 minutes  | 2 hours      |
| `RECOVERY`    | 8 hours     | 8 hours      |
| `SERVICE`     | 24 hours    | 7 days       |

Re-authentication produces a new session id; sessions are not refreshed in place.

## 9. Capability binding skeleton

(Full vault broker API is in `02_vault_broker.md`, deferred to its own spec.)

```proto
message CapabilityBinding {
  string capability_id = 1;                    // cap_<ulid>
  string subject_canonical_id = 2;
  string group_id = 3;                          // scope: capability is only active in this group
  string vault_capability_id = 4;               // reference into vault broker's catalog
  google.protobuf.Timestamp granted_at = 5;
  google.protobuf.Timestamp expires_at = 6;
  string granted_by = 7;                        // canonical_subject_id of granter
  string approval_id = 8;                       // links to approval evidence
  bool is_one_shot = 9;                          // true → revoked after first use
}
```

The identity service maintains the `CapabilityBinding` set per subject. Use of a capability is mediated by the vault broker, which checks both binding validity and the active session's `primary_group_id == binding.group_id`.

## 10. AI subject discipline

### 10.1 `is_ai` is constitutional

`is_ai = true` for any subject with `SubjectKind = AI_AGENT`. Other kinds carry `is_ai = false` even if they internally use LLMs. The identity service signs the `is_ai` field; subjects cannot tamper with it.

### 10.2 AI self-approval prevention

S2.3 §17 prevents AI subjects from approving their own actions. This spec adds the upstream invariant: an AI subject cannot enter a session whose `session_class = RECOVERY`. The identity service rejects such authentication attempts with `AISubjectCannotEnterRecovery`. Combined with S2.3 §26.2.3 `AISystemAdminBlocked`, AI subjects have no path to system mutation.

### 10.3 Risk flag computation for AI subjects

AI subjects always carry at least the `ELEVATED_PRIVILEGE_REQUEST` risk flag when their action target falls outside their own scope (`groups/<g>/agents/<a>/` for group agent, `groups/<g>/users/<u>/agents/<a>/` for personal agent). This enables S2.3 to require approval for any AI action that touches paths outside the agent's home scope.

### 10.4 AI agent retirement and replacement

When an AI agent is replaced (new model, new system prompt, new capabilities), the new version receives a **new canonical id** even if the agent_id segment is unchanged. The version is captured in the identity bundle. Old version's actions remain traceable via the old canonical id. This is consistent with the immutability rule in §4.4.

## 11. Subject normalization for S2.3

S2.3 §3 requires the policy kernel to receive a normalized subject record on every action. The identity service produces this record:

```proto
message NormalizedSubject {
  string canonical_subject_id = 1;
  SubjectKind kind = 2;
  bool is_ai = 3;
  string primary_group_id = 4;
  repeated string memberships = 5;             // all group_ids the subject belongs to
  repeated string capabilities = 6;             // capability_ids active in this session
  SessionClass session_class = 7;
  bool recovery_mode = 8;
  repeated RiskFlag risk_flags = 9;
  string identity_bundle_version = 10;
  bytes ed25519_signature = 11;                // identity service signs
}
```

The Capability Runtime fetches this record at envelope acceptance and embeds it into the `PolicyEvaluationRequest`. The signature ensures the policy kernel cannot be deceived by a forged subject record.

## 12. Determinism contract

```text
GIVEN
  canonical_subject_id     = S
  primary_group_id          = G
  identity_bundle_version   = idbundle_B
  vault_binding_set_version = vbset_V
  session_class             = SC
  recovery_mode             = RM

THEN
  ResolveSubject(S, G, B, V, SC, RM) ≡ ResolveSubject(...)
  for the same input tuple.
```

The output `NormalizedSubject` is fully determined by the input. The identity service may cache resolutions per `(canonical_subject_id, primary_group_id, idbundle_version, vbset_version, session_class, recovery_mode)`; cache invalidation occurs on bundle or vault binding set version change.

## 13. Performance contract

| Operation                 | p50      | p95      | p99      | Hard timeout |
| ------------------------- | -------- | -------- | -------- | ------------ |
| `AuthenticateSubject`     | < 50 ms  | < 200 ms | < 500 ms | 5 s          |
| `ResolveSubject` (cached) | < 100 µs | < 500 µs | < 2 ms   | 100 ms       |
| `ResolveSubject` (fresh)  | < 1 ms   | < 5 ms   | < 20 ms  | 200 ms       |
| `EnumerateMemberships`    | < 200 µs | < 1 ms   | < 5 ms   | 100 ms       |
| `SwitchPrimaryGroup`      | < 100 ms | < 500 ms | < 2 s    | 10 s         |
| `IssueCapabilityBinding`  | < 50 ms  | < 200 ms | < 1 s    | 5 s          |

Failure modes — all fail closed:

- `IdentityServiceInternal` → caller receives error; engine emits alert.
- `IdentityBundleSignatureFailure` → identity service enters degraded mode (only constitutional `_system` subjects available).
- `IdentityBundleUnavailable` → fail closed; cached subjects served until cache TTL expires; new authentications rejected.

## 14. Adversarial robustness

### 14.1 Forged subject

`NormalizedSubject` carries an Ed25519 signature by the identity service. The policy kernel verifies the signature at every evaluation. A forged subject (e.g., an attacker setting `is_ai = false` on an AI agent) fails verification.

### 14.2 Replay protection

`Session.session_id` is unique per authentication. A captured session token cannot be replayed because the identity service checks `session_id` against the active set; revoked or expired sessions are rejected at envelope acceptance.

### 14.3 Group-id collision

Group registration is content-addressed by the identity bundle. Two simultaneous registrations of the same `group_id` resolve via S1.3 optimistic concurrency at the storage layer; one wins, the other receives `GroupAlreadyExists`.

### 14.4 Capability binding leak

Bindings are scoped to `(subject_canonical_id, group_id, identity_bundle_version)`. A binding issued under one bundle version is invalidated when the bundle rolls over (new version) — the vault broker re-validates per the active version. This bounds the impact of a leaked binding to the lifetime of one bundle version.

### 14.5 AI subject impersonating human

The identity service refuses to issue a session where `kind = AI_AGENT` produces a `NormalizedSubject` with `is_ai = false`. The flag is bound to the subject record at registration and signed; the session inherits it.

### 14.6 Recovery mode escalation

There is no path from a normal-mode session to a recovery-mode session except by reboot into recovery. The identity service rejects any request to elevate `session_class` to `RECOVERY` for an existing session.

### 14.7 Membership enumeration leak

`EnumerateMemberships` returns only the calling subject's own memberships. It does not return other subjects' memberships, even to group admins. Cross-subject membership inspection requires `system_audit_read` capability + recovery mode.

## 15. Cross-spec dependencies

| Spec                               | Direction  | What this spec contributes                                                                                                    |
| ---------------------------------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------- |
| S0.1                               | producer   | `subject` field in action envelope is `canonical_subject_id`; `subject_signature` field validated by Capability Runtime       |
| S2.1                               | producer   | `subject.primary_group_id`, `subject.canonical_subject_id`, `subject.is_ai` are queryable closed fields                       |
| S2.3                               | producer   | `NormalizedSubject` consumed by policy kernel §3 normalization stage; AI self-approval prevention enforced through `is_ai`    |
| S2.4                               | producer   | property `POLICY_AI_SELF_APPROVAL_BLOCKED` audits `is_ai` linkage with policy decisions                                       |
| S3.1                               | producer   | Records carry `subject_canonical_id`; new record types `GROUP_REGISTERED` and `PRIMARY_GROUP_SWITCHED` added                  |
| S3.2                               | producer   | `subject.is_ai` and `subject.recovery_mode` drive runtime safety floor selection (§5.2)                                       |
| S4.1                               | producer   | Subject canonical id is `<group_id>:<sub_id>` per §4; group is first-class identity per §5; primary_group_id semantics per §6 |
| S6.1 vault broker (deferred)       | constraint | capability bindings scoped to `(subject, group_id, bundle_version)`                                                           |
| S6.2 approval mechanics (deferred) | constraint | approvals bound to `subject_canonical_id` + `request_hash` per S0.1 §4                                                        |
| S6.3 emergency override (deferred) | constraint | overrides require `kind = HUMAN_USER` + `recovery_mode = false` + STRONG session class                                        |

## 16. Golden fixtures

### Fixture 1 — Personal household authentication

```text
Setup:
  group: family
  human user: alice (kind=HUMAN_USER)
  AI agent: family-assistant (kind=AI_AGENT, is_ai=true, group-owned)
  alice memberships: [family]

AuthenticateSubject for alice (password + WebAuthn):
  Result: Session{
    subject_canonical_id = "family:alice",
    primary_group_id = "family",
    session_class = STRONG,
    is_ai = false,
    recovery_mode = false,
    risk_flags = []
  }

AuthenticateSubject for family-assistant (agent runtime token):
  Result: Session{
    subject_canonical_id = "family:family-assistant",
    primary_group_id = "family",
    session_class = SERVICE,
    is_ai = true,
    recovery_mode = false,
    risk_flags = [ELEVATED_PRIVILEGE_REQUEST]
  }
```

### Fixture 2 — Multi-group user with primary switch

```text
Setup:
  groups: personal, work-finance
  alice memberships: [personal (primary), work-finance]

Initial authentication:
  Session.primary_group_id = "personal"
  canonical_subject_id = "personal:alice"

SwitchPrimaryGroup(target="work-finance"):
  Old session closed; PRIMARY_GROUP_SWITCHED evidence emitted.
  New session:
    subject_canonical_id = "work-finance:alice"
    primary_group_id = "work-finance"
  Capabilities granted under "personal:alice" are NOT active.
```

### Fixture 3 — Recovery operator session

```text
Boot into recovery-safe kernel.
Authenticate via signed operator credential.

Result: Session{
  subject_canonical_id = "_system:local:operator-247",
  primary_group_id = "_system",
  session_class = RECOVERY,
  is_ai = false,
  recovery_mode = true,
  expires_at = authenticated_at + 8h (hard cap)
}

Attempting to extend the session: rejected with `RecoverySessionNotExtendable`.
```

### Fixture 4 — AI agent rejected from recovery

```text
Attempt: AuthenticateSubject for "family:family-assistant" with session_class = RECOVERY

Result: AuthenticationError{ code = AISubjectCannotEnterRecovery }
```

### Fixture 5 — Forged subject signature

```text
Capability Runtime receives action envelope with:
  subject_canonical_id = "family:family-assistant"
  is_ai = false              # tampered
  subject_signature = <not matching identity service public key>

Result: envelope rejected with `SubjectSignatureInvalid` at acceptance time.
Evidence: SUBJECT_SIGNATURE_FAILURE record (FOREVER retention; new record type added to S3.1 RecordType vocabulary as part of this contract's adoption).
```

### Fixture 6 — Capability binding scope enforcement

```text
Setup:
  alice has capability cap_X granted under "family:alice" with group_id="family"

Action 1: alice operates under "family:alice"
  → cap_X is active; vault broker honors it.

Action 2: alice switches to "homelab:alice"
  → cap_X is NOT active in this session.
  Vault broker rejects use with `CapabilityNotActiveInGroup`.
```

## 17. Telemetry contract

All metrics MUST use bounded label cardinality. **canonical_subject_id, group_id, user_id, session_id are NEVER labels.**

| Metric                                         | Type      | Labels (closed)                                              |
| ---------------------------------------------- | --------- | ------------------------------------------------------------ |
| `identity_authenticate_total`                  | counter   | `kind` (closed enum), `result` (success/error), `error_code` |
| `identity_authenticate_duration_seconds`       | histogram | `kind`, `result`                                             |
| `identity_resolve_subject_total`               | counter   | `result`, `cache` (hit/miss)                                 |
| `identity_active_sessions`                     | gauge     | `session_class` (closed enum)                                |
| `identity_primary_group_switch_total`          | counter   | none                                                         |
| `identity_capability_binding_issued_total`     | counter   | `is_one_shot` (true/false)                                   |
| `identity_capability_binding_revoked_total`    | counter   | `reason` (closed enum)                                       |
| `identity_subject_signature_failure_total`     | counter   | `kind` (closed enum)                                         |
| `identity_recovery_session_total`              | counter   | `result` (success/expired/rejected)                          |
| `identity_ai_subject_recovery_rejection_total` | counter   | none                                                         |
| `identity_bundle_load_total`                   | counter   | `result` (success/signature_failure/parse_failure)           |

Cardinality budget: ≤ 100 active label tuples per metric. The closed enums together produce fewer than 80 distinct tuples across all metrics.

## 18. Acceptance criteria

- [ ] `SubjectKind` is a closed enum with seven values; adding a value requires a versioned spec change.
- [ ] Canonical subject id matches the regex in §4.2 and the kind-specific format in §4.1.
- [ ] Group is registered only via recovery-mode action; signed `GROUP_REGISTERED` evidence with FOREVER retention.
- [ ] Multi-group subjects have exactly one `primary_group_id` per session; `SwitchPrimaryGroup` requires re-authentication.
- [ ] Recovery-mode subjects always carry `_system` scope; no path from normal mode to recovery mode except by reboot.
- [ ] Recovery sessions have an 8-hour hard cap and cannot be extended.
- [ ] `is_ai` is bound at registration and signed; subjects cannot tamper with it.
- [ ] AI subjects are rejected from `RECOVERY` session class.
- [ ] Capability bindings are scoped to `(subject, group_id, identity_bundle_version)`; vault broker enforces at use.
- [ ] `NormalizedSubject` carries an Ed25519 signature; policy kernel verifies on every evaluation.
- [ ] Identity bundle signature failure puts service into degraded mode (only constitutional `_system` subjects).
- [ ] All six golden fixtures (§16) produce the specified outcomes.
- [ ] Telemetry conforms to §17 cardinality bounds; subject/group/user/session ids never appear as labels.

## 19. Open deferrals

These are intentionally out of scope for S5.1 and tracked elsewhere:

- **Vault broker API** (`02_vault_broker.md`) — full capability operations, secret classes, use-without-reveal; this spec only defines the binding skeleton.
- **Approval mechanics** (`04_approval_mechanics.md`) — delivery channels, signed approvals, request-hash binding.
- **Emergency override mechanics** (`05_emergency_override.md`) — full mechanics behind the boundary set in S2.3 §16.
- **Cross-machine identity federation** — multi-host AIOS clusters; deferred.
- **Subject delegation** (alice authorizes bob to act on her behalf) — deferred to future spec; needs careful consent and revocation semantics.
- **Hardware-pinned device attestation** (TPM-bound device identity) — deferred to L8 hardware integration.
- **Group nesting / subgroups** — deferred per S4.1 §6 default Q2.
- **Identity bundle hot-reload mid-session** — current contract: bundle change applies to new sessions only; existing sessions continue under the version they authenticated against until expiry.
- **Multi-tenant identity isolation** (`tenants/` instead of `groups/`) — deferred per S4.1 Q1 default.
- **Webauthn / passkey full integration** — referenced as the credential mechanism for STRONG session class but the protocol details are deferred to a deployment guide.

## Appendix A — Full Proto IDL

```proto
syntax = "proto3";
package aios.identity.v1alpha1;

import "google/protobuf/timestamp.proto";

// ============================================================================
// Service
// ============================================================================

service IdentityService {
  // Authenticate a subject and issue a Session.
  rpc AuthenticateSubject(AuthenticateSubjectRequest) returns (AuthenticateSubjectResponse);

  // Produce a NormalizedSubject record for the policy kernel.
  rpc ResolveSubject(ResolveSubjectRequest) returns (ResolveSubjectResponse);

  // List the active subject's group memberships.
  rpc EnumerateMemberships(EnumerateMembershipsRequest) returns (EnumerateMembershipsResponse);

  // Switch the active session's primary_group_id; closes old session and issues new.
  rpc SwitchPrimaryGroup(SwitchPrimaryGroupRequest) returns (SwitchPrimaryGroupResponse);

  // Issue a capability binding (mediated by vault broker; this RPC creates the binding record).
  rpc IssueCapabilityBinding(IssueCapabilityBindingRequest) returns (IssueCapabilityBindingResponse);

  // Revoke a capability binding.
  rpc RevokeCapabilityBinding(RevokeCapabilityBindingRequest) returns (RevokeCapabilityBindingResponse);

  // Engine info: bundle version, schema version, active session count.
  rpc GetIdentityInfo(GetIdentityInfoRequest) returns (GetIdentityInfoResponse);
}

// ============================================================================
// Core types
// ============================================================================

enum SubjectKind {
  SUBJECT_KIND_UNSPECIFIED = 0;
  HUMAN_USER = 1;
  AI_AGENT = 2;
  APPLICATION = 3;
  SERVICE = 4;
  DEVICE = 5;
  WORKFLOW = 6;
  REMOTE_OPERATOR = 7;
}

enum SessionClass {
  SESSION_CLASS_UNSPECIFIED = 0;
  PUBLIC = 1;
  INTERACTIVE = 2;
  STRONG = 3;
  RECOVERY = 4;
  SERVICE = 5;
}

enum GroupTier {
  GROUP_TIER_UNSPECIFIED = 0;
  PERSONAL = 1;
  TEAM = 2;
  ORGANIZATIONAL = 3;
  FINANCE = 4;
}

enum RiskFlagKind {
  RISK_FLAG_KIND_UNSPECIFIED = 0;
  AUTH_FROM_NEW_DEVICE = 1;
  AUTH_FROM_NEW_LOCATION = 2;
  RECENT_FAILED_ATTEMPTS = 3;
  PASSWORD_RECENTLY_CHANGED = 4;
  SUBJECT_RECENTLY_CREATED = 5;
  OFF_HOURS = 6;
  ELEVATED_PRIVILEGE_REQUEST = 7;
}

message Subject {
  string canonical_subject_id = 1;
  SubjectKind kind = 2;
  bool is_ai = 3;
  string display_name = 4;
  google.protobuf.Timestamp created_at = 5;
  string created_by = 6;
  string identity_bundle_version = 7;
  bytes ed25519_signature = 8;
}

message Group {
  string group_id = 1;
  string display_name = 2;
  google.protobuf.Timestamp created_at = 3;
  string created_by = 4;
  GroupTier tier = 5;
  repeated string admins = 6;
  bool can_have_ai_agents = 7;
  bool can_install_apps = 8;
  bool federation_eligible = 9;
  string identity_bundle_version = 10;
}

message Membership {
  string subject_canonical_id = 1;
  string group_id = 2;
  google.protobuf.Timestamp joined_at = 3;
  string joined_via = 4;
  repeated string roles = 5;
  bool is_primary = 6;
}

message RiskFlag {
  RiskFlagKind kind = 1;
  string detail = 2;
}

message Session {
  string session_id = 1;
  string subject_canonical_id = 2;
  string primary_group_id = 3;
  SessionClass session_class = 4;
  bool is_ai = 5;
  bool recovery_mode = 6;
  google.protobuf.Timestamp authenticated_at = 7;
  google.protobuf.Timestamp expires_at = 8;
  repeated RiskFlag risk_flags = 9;
  string identity_bundle_version = 10;
  bytes ed25519_signature = 11;
}

message NormalizedSubject {
  string canonical_subject_id = 1;
  SubjectKind kind = 2;
  bool is_ai = 3;
  string primary_group_id = 4;
  repeated string memberships = 5;
  repeated string capabilities = 6;
  SessionClass session_class = 7;
  bool recovery_mode = 8;
  repeated RiskFlag risk_flags = 9;
  string identity_bundle_version = 10;
  bytes ed25519_signature = 11;
}

message CapabilityBinding {
  string capability_id = 1;
  string subject_canonical_id = 2;
  string group_id = 3;
  string vault_capability_id = 4;
  google.protobuf.Timestamp granted_at = 5;
  google.protobuf.Timestamp expires_at = 6;
  string granted_by = 7;
  string approval_id = 8;
  bool is_one_shot = 9;
}

// ============================================================================
// RPC request/response
// ============================================================================

message AuthenticateSubjectRequest {
  string canonical_subject_id_hint = 1;       // optional; resolver can derive from credentials
  bytes credentials = 2;                       // password hash, token, cert depending on kind
  string requested_primary_group_id = 3;
  SessionClass requested_session_class = 4;
  string device_id = 5;                         // optional
}

message AuthenticateSubjectResponse {
  oneof result {
    Session session = 1;
    AuthenticationError error = 2;
  }
}

enum AuthenticationErrorCode {
  AUTHENTICATION_ERROR_CODE_UNSPECIFIED = 0;
  CREDENTIALS_INVALID = 1;
  SUBJECT_RETIRED = 2;
  SUBJECT_LOCKED = 3;
  PRIMARY_GROUP_NOT_MEMBER = 4;
  AI_SUBJECT_CANNOT_ENTER_RECOVERY = 5;
  IDENTITY_BUNDLE_DEGRADED = 6;
  RATE_LIMITED = 7;
  SUBJECT_SIGNATURE_INVALID = 8;
  SESSION_CLASS_NOT_AVAILABLE = 9;
}

message AuthenticationError {
  AuthenticationErrorCode code = 1;
  string message = 2;
}

message ResolveSubjectRequest {
  string session_id = 1;
}

message ResolveSubjectResponse {
  oneof result {
    NormalizedSubject subject = 1;
    string error_message = 2;
  }
}

message EnumerateMembershipsRequest { string session_id = 1; }
message EnumerateMembershipsResponse { repeated Membership memberships = 1; }

message SwitchPrimaryGroupRequest {
  string session_id = 1;
  string target_group_id = 2;
  bytes credentials = 3;
}
message SwitchPrimaryGroupResponse {
  oneof result {
    Session new_session = 1;
    AuthenticationError error = 2;
  }
}

message IssueCapabilityBindingRequest {
  string session_id = 1;
  string subject_canonical_id = 2;
  string vault_capability_id = 3;
  google.protobuf.Timestamp expires_at = 4;
  string approval_id = 5;
  bool is_one_shot = 6;
}
message IssueCapabilityBindingResponse {
  CapabilityBinding binding = 1;
}

message RevokeCapabilityBindingRequest { string capability_id = 1; string reason = 2; }
message RevokeCapabilityBindingResponse { bool revoked = 1; }

message GetIdentityInfoRequest {}
message GetIdentityInfoResponse {
  string identity_bundle_version = 1;
  string schema_version = 2;            // "aios.identity.v1alpha1"
  uint64 active_session_count = 3;
  uint64 active_subject_count = 4;
  uint64 active_group_count = 5;
  bool degraded_mode = 6;
  google.protobuf.Timestamp bundle_loaded_at = 7;
}
```

## See also

- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S2.3 — Policy Kernel](01_policy_kernel.md)
- [S4.1 — Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [Vault Broker — `02_vault_broker.md`](02_vault_broker.md) (deferred)
- [Approval Mechanics — `04_approval_mechanics.md`](04_approval_mechanics.md) (deferred)
- [Emergency Override — `05_emergency_override.md`](05_emergency_override.md) (deferred)
- [L4 Overview](00_overview.md)
- [Rev.1 §6 — Subject Taxonomy](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
