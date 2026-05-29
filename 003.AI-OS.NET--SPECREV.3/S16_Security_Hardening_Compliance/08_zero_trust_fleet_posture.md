# S16.8 — Zero-Trust Fleet Posture

| Field     | Value                                                                                                                                                                                                               |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-29; awaiting implementation evidence)                                                                                                                                                   |
| Phase tag | S16.8                                                                                                                                                                                                               |
| Layer     | Cross-cutting: L8, L4, L9                                                                                                                                                                                           |
| Consumes  | S8.1 Network Policy, S5.1 Identity Model, S2.3 Policy Kernel, S3.1 Evidence Log, S16.1 Security Profile Matrix, S16.4 Measured Boot + Runtime Integrity                                                             |
| Produces  | `ZeroTrustPosture`, `ContinuousPostureCheck`, `PerRequestAuthorizationDecision`, `DeviceTrustState`, `PostureSignalKind` enum, `TrustTier` enum, `PostureDowngradeReason` enum, zero-trust posture evidence records |

## 1. Responsibility

S16.8 defines the **zero-trust posture rules** that govern access between AIOS
hosts when more than one host is present (cluster / fleet mode). It is the
AIOS realization of the NIST SP 800-207 zero-trust principles: there is no
implicit trust granted by network location, every access decision is made
per-request against fresh signed state, and the trust standing of every device
and subject is continuously re-evaluated rather than assumed at connection
time.

This contract owns the **posture model** only:

- what signals make up a `ZeroTrustPosture`,
- how a `DeviceTrustState` is computed and downgraded,
- how a `PerRequestAuthorizationDecision` is reached,
- how `ContinuousPostureCheck` re-evaluation runs,
- which evidence each of these emits.

It does **not** own cluster mechanics. The fleet/cluster membership, federated
identity, hub-and-spoke WireGuard overlay, Merkle-DAG evidence replication, and
remote-execution routing are owned by **S25 Fleet, Cluster, and Remote
Execution** (a Rev.3 future contract, holistic §14). S16.8 supplies S25 with
the posture verdict; S25 supplies S16.8 with membership and transport facts.
The boundary is stated explicitly in §3 to avoid duplication.

Invariant links: INV-002, INV-004, INV-005, INV-008, INV-012, INV-013,
INV-014, INV-017, INV-024, INV-026.

## 2. Product principle

In a single-host AIOS install there is no fleet and no network trust surface to
defend; S16.8 is `NOT_APPLICABLE` (see §9). The moment a second host joins, the
LAN must stop being a trust boundary.

```text
access request between hosts
  -> resolve subject identity (S5.1) + device identity (this contract)
  -> read signed posture: profile, boot integrity, freshness, drift
  -> evaluate THIS request against THIS posture (no cached "session trust")
  -> Policy Kernel decision (S2.3) under the local host profile (S16.1)
  -> allow scoped + time-boxed grant, step-up, or deny with reason
  -> emit evidence
  -> continuously re-check; downgrade device trust on signal change
```

The product promise to the operator: a host never trusts a peer because it is
"on the same network." It trusts a peer for one specific request, for a bounded
time, because the peer's current signed posture earned it, and every such
decision leaves an evidence receipt.

The host stays sovereign. A cluster root can publish a baseline, but it cannot
make a host accept a peer the host's own profile and Policy Kernel would deny
(INV-026, DEC-R3-003).

## 3. Boundary with S25 (no duplication)

| Concern                                          | Owner               | S16.8 relationship                                                   |
| ------------------------------------------------ | ------------------- | -------------------------------------------------------------------- |
| Cluster membership roster, join/leave            | S25                 | Consumes membership facts as input signals.                          |
| Federated identity issuance / subject federation | S25 (built on S5.1) | Consumes the resolved federated `Subject`; never re-issues identity. |
| Hub-and-spoke / mesh WireGuard overlay           | S25                 | Consumes "is the link encrypted / which peer" as a transport fact.   |
| Merkle-DAG evidence replication across hosts     | S25                 | Emits S16.8 records into the local log; S25 replicates them.         |
| Remote workload routing / execution placement    | S25                 | Provides the per-request posture verdict that routing must respect.  |
| **Zero-trust posture rules**                     | **S16.8**           | **Owned here.**                                                      |
| **Per-request authorization verdict**            | **S16.8**           | **Owned here.**                                                      |
| **Device trust state + downgrade rules**         | **S16.8**           | **Owned here.**                                                      |
| **Continuous posture re-check policy**           | **S16.8**           | **Owned here.**                                                      |

Reading rule: when S25 needs to decide whether peer B may act on host A, S25
calls the S16.8 posture evaluation. S16.8 never opens sockets, never manages
the overlay, and never edits the membership roster.

## 4. Reference patterns

| Pattern                                                                                           | S16.8 use                                                                                                                                               |
| ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [NIST SP 800-207 Zero Trust Architecture](https://csrc.nist.gov/pubs/sp/800/207/final)            | Foundational model: no implicit network trust, per-request authorization, continuous diagnostics, dynamic policy from multiple signals.                 |
| [NIST SP 800-207 §3.1 tenets](https://csrc.nist.gov/pubs/sp/800/207/final)                        | The seven tenets map directly to `ZeroTrustPosture` signals and `PerRequestAuthorizationDecision` inputs.                                               |
| [BeyondCorp access model](https://research.google/pubs/pub43231/)                                 | Device + user trust composed per request; access independent of network location.                                                                       |
| [NIST SP 800-53 Rev. 5 AC / CA / SI families](https://csrc.nist.gov/pubs/sp/800/53/r5/upd1/final) | Continuous monitoring (CA-7), access enforcement (AC-3), device authentication (IA-3) controls satisfied by posture re-check and per-request decisions. |
| [WireGuard CryptoKey routing](https://www.wireguard.com/protocol/)                                | Identity-bound encrypted transport assumed under cluster mode; the S25-managed overlay (DEC-R3-003).                                                    |
| [RFC 9334 RATS architecture](https://datatracker.ietf.org/doc/rfc9334/)                           | Attestation-evidence freshness vocabulary reused for `boot_integrity` and `attestation_freshness` posture signals (bound to S16.4).                     |

## 5. Core schemas

### 5.1 `ZeroTrustPosture`

The signed snapshot of a host's current standing as a fleet participant. It is
the unit a peer reads before granting any access. It is content-addressed and
recorded so a verdict can be replayed from evidence.

```yaml
zero_trust_posture:
  posture_id: "ztp_<ULID>"
  host_id: "host:<canonical>"
  generated_at: "<rfc3339>"
  not_valid_after: "<rfc3339>" # posture freshness window; expiry = stale
  security_profile: SECURE_DEFAULT # from S16.1; closed enum
  signals:
    boot_integrity: PASS # bound to S16.4 measured boot
    attestation_freshness: FRESH # PostureSignalKind freshness band
    selinux_enforcing: true # from active SecurityProfile (S16.1)
    evidence_chain_intact: true # local log hash chain verified (S3.1)
    drift_detected: false # config/kernel/hardware drift signal
    profile_floor_met: true # host profile >= fleet baseline floor
    last_continuous_check_id: "cpc_<ULID>"
  identity:
    subject_ref: "subject:<canonical>" # resolved via S5.1; never re-issued here
    federated: true # membership fact supplied by S25
  signature_chain: [] # signed by local host trust root
```

`security_profile` reuses the closed `SecurityProfile` enum from S16.1 verbatim
(`DEV_RELAXED | SECURE_DEFAULT | STIG_ALIGNED | AIRGAP_HIGH`). No new profile
values are introduced here. Unknown values are rejected by the posture loader.

### 5.2 `PostureSignalKind` (closed enum)

```text
PostureSignalKind =
  BOOT_INTEGRITY
| ATTESTATION_FRESHNESS
| MAC_ENFORCEMENT
| EVIDENCE_CHAIN_INTACT
| CONFIG_DRIFT
| KERNEL_DRIFT
| HARDWARE_DRIFT
| PROFILE_FLOOR
| NETWORK_TRANSPORT_ENCRYPTED
| SUBJECT_AUTH_FRESHNESS
```

Each signal resolves to one of the freshness/health bands
`PASS | WARN | FAIL | FRESH | STALE | EXPIRED`. Unknown signal kinds and unknown
bands are rejected by the posture validator.

### 5.3 `TrustTier` (closed enum)

Computed standing of a device, derived from its signals. Tiers are ordered;
each request requires a minimum tier set by policy.

```text
TrustTier =
  TIER_FULL        # all required signals PASS/FRESH; profile floor met
| TIER_LIMITED     # WARN signals present; read-only / low-risk actions only
| TIER_STEP_UP     # action allowed only after fresh re-auth / re-attestation
| TIER_QUARANTINED # FAIL signal or expired posture; deny all but recovery
| TIER_UNKNOWN     # no valid posture yet; treated as QUARANTINED
```

Unknown tier values are rejected by the authorization engine. `TIER_UNKNOWN`
and `TIER_QUARANTINED` both fail closed.

### 5.4 `DeviceTrustState`

The continuously-maintained trust record for one peer device, held by each
host about each peer it interacts with. This is the object that gets
**downgraded** by `ContinuousPostureCheck`.

```yaml
device_trust_state:
  device_trust_id: "dts_<ULID>"
  host_id: "host:<canonical>" # the device this state describes
  observer_host_id: "host:<canonical>" # the host holding this opinion
  current_tier: TIER_FULL
  posture_ref: "ztp_<ULID>" # posture this tier was computed from
  last_evaluated_at: "<rfc3339>"
  next_recheck_no_later_than: "<rfc3339>"
  active_signals:
    boot_integrity: PASS
    attestation_freshness: FRESH
    config_drift: PASS
  downgrade_history: # append-only locally; never silently cleared
    - at: "<rfc3339>"
      from_tier: TIER_FULL
      to_tier: TIER_STEP_UP
      reason: ATTESTATION_STALE
      evidence_receipt_id: "evr_..."
  recovery_exempt: false # recovery path never depends on peer trust (INV-001)
```

A device's tier is never silently _upgraded_ by time alone; an upgrade requires
a fresh posture that passes evaluation. A downgrade is immediate on any
qualifying signal change.

### 5.5 `PostureDowngradeReason` (closed enum)

```text
PostureDowngradeReason =
  ATTESTATION_STALE
| ATTESTATION_FAILED
| BOOT_INTEGRITY_FAILED
| MAC_NOT_ENFORCING
| EVIDENCE_CHAIN_BROKEN
| CONFIG_DRIFT_DETECTED
| KERNEL_DRIFT_DETECTED
| HARDWARE_DRIFT_DETECTED
| PROFILE_FLOOR_VIOLATED
| POSTURE_EXPIRED
| TRANSPORT_NOT_ENCRYPTED
| SUBJECT_REAUTH_REQUIRED
| OPERATOR_QUARANTINE
```

Unknown reasons are rejected by the downgrade evaluator. Every downgrade carries
exactly one of these reasons and an evidence receipt.

### 5.6 `ContinuousPostureCheck`

The scheduled / event-driven re-evaluation that keeps `DeviceTrustState`
current. NIST 800-207 "continuous diagnostics and mitigation" realized as a
typed loop, not a one-time handshake.

```yaml
continuous_posture_check:
  check_id: "cpc_<ULID>"
  observer_host_id: "host:<canonical>"
  target_host_id: "host:<canonical>"
  trigger: SCHEDULED # CheckTrigger; closed enum
  posture_examined: "ztp_<ULID>"
  signals_evaluated:
    - kind: BOOT_INTEGRITY
      band: PASS
    - kind: ATTESTATION_FRESHNESS
      band: STALE
  resulting_tier: TIER_STEP_UP
  tier_changed: true
  downgrade_reason: ATTESTATION_STALE # null if no downgrade
  evidence_receipt_id: "evr_..."
```

```text
CheckTrigger =
  SCHEDULED
| ON_FIRST_CONTACT
| ON_PROFILE_CHANGE
| ON_DRIFT_SIGNAL
| ON_ATTESTATION_REFRESH
| ON_OPERATOR_REQUEST
```

Unknown triggers are rejected by the scheduler. Re-check cadence floors are set
per profile in §9.

### 5.7 `PerRequestAuthorizationDecision`

The core NIST 800-207 artifact: **every** inter-host access is authorized
per-request against fresh state. There is no long-lived "trusted session." A
prior allow does not authorize the next request.

```yaml
per_request_authorization_decision:
  decision_id: "prad_<ULID>"
  request_ref: "req_<ULID>"
  requester:
    subject_ref: "subject:<canonical>" # resolved via S5.1
    device_trust_ref: "dts_<ULID>"
    presented_tier: TIER_FULL
  target:
    host_id: "host:<canonical>"
    resource: "aios://..." # the action target, scoped
    requested_action: "typed-action-id" # never free-form shell (INV-002 / S10.1)
  inputs:
    local_security_profile: SECURE_DEFAULT
    fleet_baseline_floor: SECURE_DEFAULT
    posture_ref: "ztp_<ULID>"
    required_tier: TIER_FULL
    network_transport_encrypted: true # transport fact from S25 overlay
  verdict: ALLOW # AuthorizationVerdict; closed enum
  granted_scope:
    actions: ["typed-action-id"]
    not_valid_after: "<rfc3339>" # time-boxed; no standing trust
    one_shot: true # decision authorizes THIS request only
  denial:
    reason_code: null # PostureDenyReason when verdict != ALLOW
    human_readable: null
  policy_decision_ref: "pol_..." # the S2.3 Policy Kernel decision id
  evidence_receipt_id: "evr_..."
```

```text
AuthorizationVerdict =
  ALLOW
| ALLOW_WITH_STEP_UP
| DENY
| DENY_QUARANTINED
```

```text
PostureDenyReason =
  TIER_BELOW_REQUIRED
| POSTURE_STALE
| PROFILE_FLOOR_VIOLATED
| TRANSPORT_NOT_ENCRYPTED
| SUBJECT_UNVERIFIED
| HOST_POLICY_DENY        # local host profile/Policy Kernel said no (INV-026)
| ACTION_NOT_TYPED        # free-form / non-typed action attempted
| AI_AUTHORITY_DENIED     # AI subject attempted to grant/approve fleet access
```

Unknown verdicts and unknown deny reasons are rejected by the authorization
engine. The default on any unrecognized or missing input is `DENY` (fail
closed).

## 6. Per-request authorization flow

S16.8 reuses the universal request → state → policy → evidence pattern; it does
not invent a parallel one. There is no candidate-generation/promotion step
because authorization is a verdict, not a mutation, but the inspect-signed-state
→ policy → evidence spine is identical.

```text
inter-host request
  -> resolve federated subject identity (S5.1, membership from S25)
  -> read peer DeviceTrustState + its ZeroTrustPosture (signed, fresh)
  -> if posture stale/expired -> trigger ContinuousPostureCheck (ON_FIRST_CONTACT)
  -> compute presented TrustTier from current signals
  -> compare presented tier vs required tier for the typed action
  -> apply local Policy Kernel decision under the LOCAL host profile (S16.1)
       (cluster baseline can RAISE the floor, never LOWER it — INV-026)
  -> verdict: ALLOW | ALLOW_WITH_STEP_UP | DENY | DENY_QUARANTINED
  -> if ALLOW_WITH_STEP_UP: require fresh re-auth / re-attestation, then re-decide
  -> emit PER_REQUEST_AUTH_DECISION evidence (append-only)
  -> grant is scoped to one typed action and time-boxed; no standing session
```

Two hard rules sit on this flow:

- **No network-location trust.** Being reachable over the LAN or the cluster
  overlay grants nothing. The transport being encrypted is one _input_ signal,
  not an authorization.
- **Local sovereignty wins.** If the local host's profile or Policy Kernel
  denies, the verdict is `DENY` regardless of any cluster-root baseline. A
  cluster baseline may only make the local decision _stricter_ (INV-026,
  DEC-R3-003).

## 7. Device trust downgrade flow

```text
ContinuousPostureCheck (scheduled or event-triggered)
  -> re-read peer ZeroTrustPosture
  -> evaluate each PostureSignalKind to a band
  -> recompute TrustTier
  -> if tier dropped:
       record DeviceTrustState.downgrade_history entry
       emit DEVICE_TRUST_DOWNGRADED with PostureDowngradeReason
       invalidate any cached per-request inputs for that peer
  -> if a FAIL signal or POSTURE_EXPIRED: tier = TIER_QUARANTINED (deny all but recovery)
```

A downgrade takes effect immediately for the _next_ per-request decision; it
cannot retroactively revoke an already-completed one-shot grant, but because
grants are one-shot and time-boxed, exposure is bounded. Recovery paths are
exempt: a host's own recovery boot (INV-001) never depends on any peer's trust
tier.

## 8. AI authority boundary

The AI authority rules apply unchanged in fleet mode:

- An AI subject (`AI_NATIVE_SUBJECT`, `AI_AGENT_CAPSULE`) **cannot** issue,
  approve, or raise a `PerRequestAuthorizationDecision`. Any such attempt yields
  `verdict = DENY` with `reason_code = AI_AUTHORITY_DENIED`.
- An AI subject **cannot** mutate a `DeviceTrustState`, clear a downgrade, or
  extend a posture freshness window. It may _read_ posture state to explain a
  blocked fleet action to the operator, and it may _propose_ a typed action that
  a human or `SYSTEM_SERVICE` then authorizes.
- AI **cannot weaken the fleet baseline floor or a host profile** to make a
  cross-host request pass (consistent with S16.1 hard denies and INV-002).
- AI **cannot author the eBPF** that some transport/observability paths use; per
  DEC-R3-005 / INV-025 it may at most request a pre-vetted signed drop-only
  template — out of scope for granting fleet access.

The result: AI explains zero-trust outcomes; it never decides them.

## 9. Security profile gates

S16.8 is **mandatory for cluster mode** and **`NOT_APPLICABLE` for a
single-host MVP** (there is no peer to authorize). When more than one host is
present, the active host `SecurityProfile` (S16.1) sets the posture floor.

| Profile          | Zero-trust fleet rule                                                                                                                                                                                                                                                                            |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `DEV_RELAXED`    | Fleet posture optional; if a peer joins, default required tier is `TIER_LIMITED`, transport-encryption signal advisory, re-check cadence relaxed. Per-request decisions still emit evidence.                                                                                                     |
| `SECURE_DEFAULT` | Fleet posture **required** once a second host joins. Required tier `TIER_FULL` for mutating actions, `TIER_LIMITED` for read-only. Encrypted transport required. `ContinuousPostureCheck` on first contact + on every drift/profile/attestation event + scheduled floor ≤ 1h.                    |
| `STIG_ALIGNED`   | Required tier `TIER_FULL` for all cross-host actions; step-up re-attestation required for high-risk typed actions. Attestation freshness window short; scheduled re-check floor ≤ 15m. Any FAIL signal quarantines the peer. Posture must bind to S16.4 measured-boot evidence.                  |
| `AIRGAP_HIGH`    | Cross-host trust only between hosts in the same signed local enclave; no off-enclave peers. Encrypted transport mandatory; offline posture evidence export required. Scheduled re-check floor ≤ 5m; `POSTURE_EXPIRED` → immediate quarantine. No cluster-root baseline may relax any host floor. |

Hard denies (Policy Kernel, all profiles where fleet mode is active):

- no implicit access from network location alone (NIST 800-207 tenet);
- no standing / long-lived cross-host session trust;
- no AI subject authorizing, approving, or raising a per-request decision;
- no cluster root lowering a host profile floor or overriding a host `DENY`
  (INV-026);
- no `ALLOW` on a stale or expired `ZeroTrustPosture`;
- no cross-host typed action over unencrypted transport under `SECURE_DEFAULT`
  or stricter;
- no free-form / non-typed action across hosts (it fails closed as
  `ACTION_NOT_TYPED`).

## 10. Evidence records

S16.8 adds these record types:

```text
POSTURE_CHECK_RESULT
PER_REQUEST_AUTH_DECISION
DEVICE_TRUST_DOWNGRADED
ZERO_TRUST_POSTURE_PUBLISHED
DEVICE_TRUST_QUARANTINED
FLEET_BASELINE_FLOOR_APPLIED
CROSS_HOST_ACCESS_DENIED
STEP_UP_REAUTH_REQUIRED
```

These records are appended to the local S3.1 Evidence Log (append-only,
INV-014); S25 replicates them across the cluster as a Merkle-DAG (DEC-R3-003)
without altering their local content.

`PER_REQUEST_AUTH_DECISION` minimum fields:

```text
decision_id
request_ref
requester_subject_ref
requester_device_trust_ref
presented_tier
target_host_id
requested_action
required_tier
local_security_profile
fleet_baseline_floor
posture_ref
network_transport_encrypted
verdict
denial_reason_code
granted_scope_not_valid_after
policy_decision_ref
evidence_receipt_id
```

## 11. Non-goals

- Do not own or duplicate cluster membership, federated identity issuance, the
  WireGuard overlay, evidence replication, or remote workload routing — those
  are S25.
- Do not grant any access based on network location, subnet, or VLAN.
- Do not introduce a long-lived cross-host "trusted session" abstraction.
- Do not let a cluster root override or weaken a host's local policy floor.
- Do not let an AI subject decide, approve, or raise a cross-host authorization.
- Do not require fleet posture on a single-host install (it is
  `NOT_APPLICABLE`).
- Do not claim NIST 800-207 / zero-trust _certification_; S16.8 provides the
  technical posture controls and audit evidence, not an assessment verdict.
- Do not re-define the `SecurityProfile` enum; reuse S16.1 verbatim.

## 12. Acceptance criteria

S16.8 is `REAL` only when:

1. `ZeroTrustPosture`, `ContinuousPostureCheck`, `PerRequestAuthorizationDecision`,
   and `DeviceTrustState` parse and reject unknown enum values
   (`PostureSignalKind`, `TrustTier`, `PostureDowngradeReason`, `CheckTrigger`,
   `AuthorizationVerdict`, `PostureDenyReason`).
2. On a single-host install, S16.8 reports `NOT_APPLICABLE` and emits no
   cross-host authorization records.
3. With two hosts, every inter-host access produces exactly one
   `PerRequestAuthorizationDecision` evidence record — a prior `ALLOW` never
   authorizes a subsequent request.
4. An access attempt that relies only on network reachability (valid transport,
   no qualifying posture) is denied with `TIER_BELOW_REQUIRED` or
   `SUBJECT_UNVERIFIED`.
5. A stale or expired `ZeroTrustPosture` cannot yield `ALLOW`; it yields
   `POSTURE_STALE` (or quarantine on expiry).
6. A qualifying signal change drops `DeviceTrustState.current_tier` and emits
   `DEVICE_TRUST_DOWNGRADED` with a closed `PostureDowngradeReason` and a receipt
   before the next per-request decision uses the new tier.
7. `ContinuousPostureCheck` runs at or below the per-profile cadence floor and on
   each defined trigger, recording `POSTURE_CHECK_RESULT`.
8. A local host `DENY` (profile or Policy Kernel) overrides any cluster-root
   baseline; a cluster baseline can only raise the required tier, never lower a
   host floor (INV-026).
9. An AI subject attempting to issue, approve, or raise a
   `PerRequestAuthorizationDecision`, or to mutate a `DeviceTrustState`, is
   denied with `AI_AUTHORITY_DENIED`.
10. Under `STIG_ALIGNED`/`AIRGAP_HIGH`, a cross-host typed action over
    unencrypted transport is denied with `TRANSPORT_NOT_ENCRYPTED`, and any FAIL
    signal quarantines the peer.

## 13. See also

- [S16.1 Security Profile Matrix](01_security_profile_matrix.md)
- [S16.3 STIG/NIST Control Map + Scanner](03_stig_nist_control_map_scanner.md)
- [S8.1 Network Policy](../../002.AI-OS.NET--SPECREV.2/L8_Network_Hardware_Devices/02_network_policy.md)
- [S5.1 Identity Model](../../002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/03_identity_model.md)
- [S2.3 Policy Kernel](../../002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S3.1 Evidence Log](../../002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/01_evidence_log.md)
- S25 Fleet, Cluster, and Remote Execution (Rev.3 future contract; holistic §14) — owns cluster mechanics S16.8 coordinates with.
- [Rev.3 Holistic Specification](../00_REV3_HOLISTIC_SPEC.md)
