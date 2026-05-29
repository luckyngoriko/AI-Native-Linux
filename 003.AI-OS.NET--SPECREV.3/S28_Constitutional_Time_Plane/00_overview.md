# S28 - Constitutional Time Plane

| Field     | Value                                                                                                                                                                                                                                                                                                                                     |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-29; awaiting implementation evidence)                                                                                                                                                                                                                                                                         |
| Phase tag | S28                                                                                                                                                                                                                                                                                                                                       |
| Layer     | Cross-cutting: L0, L1, L9, crossing L8                                                                                                                                                                                                                                                                                                    |
| Consumes  | S3.1 Evidence Log (timestamps, hash chain, TAI64N ordering), S8.1 Network Policy (NTP/Roughtime egress grants), S9.1 Recovery Boundary (no-trusted-time boot path), S2.3 Policy Kernel (consequential-action gating), S6.3 Evidence Receipt Schema (`emitted_at`/`tai64n` envelope fields), S16.1 Security Profile Matrix (profile gates) |
| Produces  | `TrustedTimeSource`, `TimeTrustGrade`, `ClockSkewDetector`, `SkewBudget`, `TimeAttestation`, `TimePosture`, time-trust evidence record types, invariant candidate INV-034                                                                                                                                                                 |

## 1. Responsibility

S28 defines how AIOS knows **what time it is** and **how much it trusts that
knowledge**. Until Rev.3, every timestamp in the system — every evidence receipt,
every policy decision, every approval expiry, every TLS/Secure Boot validity
check — was taken on faith from `Utc::now()` reading the local clock. A wrong
clock is not a cosmetic bug: it silently invalidates certificate windows, lets
expired approvals look fresh, lets replayed evidence look in-order, and lets an
attacker who can move the clock forge the time half of any audit trail.

S28 turns time into a constitutional plane. Time acquires a **trust grade**, the
same way packages acquire a trust level and drivers acquire a taint state. Every
evidence timestamp must declare whether it came from an untrusted local clock or
from an attested source. When the clock cannot be trusted, consequential typed
actions are downgraded or blocked, and the loss of trust is itself evidenced.

This plane does not invent a new clock. It binds existing time sources (RTC, NTP,
Roughtime, TPM tick counter, GNSS) into one closed, signed posture object and
makes the Policy Kernel and Evidence Log read that object before they trust a
timestamp.

Invariant links: INV-005 (evidence append-only), INV-008 (default deny),
INV-012 (recovery required for system mutation), INV-014 (no proof, no
completion), INV-017 (security profile not silently weakened), and the new
candidate INV-034 (§13).

## 2. Product principle

A timestamp without a trust grade is a rumor, not evidence.

```text
boot
  -> read local clock (untrusted)
  -> attempt trusted time sources by profile
  -> compute skew against each source
  -> select TrustedTimeSource + TimeTrustGrade
  -> stamp TimePosture (signed)
  -> evidence timestamps carry their grade
  -> consequential actions gated on grade + skew budget
  -> recovery if no trusted time and profile requires it
```

The operator never has to read clock logs. They see a single posture: "Time is
attested (Roughtime + TPM tick), skew within budget" or "Time is untrusted —
consequential actions are blocked until a trusted source is reached or recovery
is entered."

## 3. Reference patterns

| Pattern                                                                                          | S28 use                                                                                 |
| ------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------- |
| [NTS — Network Time Security (RFC 8915)](https://www.rfc-editor.org/rfc/rfc8915)                 | Authenticated NTP: cryptographically protected time transfer over NTPv4.                |
| [Roughtime (IETF draft)](https://datatracker.ietf.org/doc/draft-ietf-ntp-roughtime/)             | Signed, auditable single-roundtrip time with proof of misbehavior.                      |
| [TAI64N labels](https://cr.yp.to/libtai/tai64n.html)                                             | Monotonic constitutional-clock ordering already used by S6.3 receipts.                  |
| [TPM 2.0 time/tick model](https://trustedcomputinggroup.org/resource/tpm-library-specification/) | `TPM2_GetTime` resettable tick counter + monotonic clock for skew-resistant ordering.   |
| [chrony / NTS client](https://chrony-project.org/documentation.html)                             | Reference disciplined-clock and NTS client behavior AIOS adapts behind a typed posture. |
| [RFC 8633 — NTP BCP](https://www.rfc-editor.org/rfc/rfc8633)                                     | Operational best practice: multiple sources, sanity bounds, leap-second handling.       |

S28 adapts these patterns behind a typed posture. It does not expose a raw NTP or
Roughtime client to AI subjects; time acquisition is a `SYSTEM_SERVICE`
responsibility under network policy (§9).

## 4. Trusted time sources

```text
TrustedTimeSource =
  LOCAL_RTC
| NTP_AUTHENTICATED
| ROUGHTIME
| TPM_TICK
| GNSS
```

Unknown values are rejected by the TimePosture loader at parse time.

| Source              | What it provides                                                                                      | Trust contribution                                                                                                       |
| ------------------- | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `LOCAL_RTC`         | Battery-backed real-time clock read at boot.                                                          | Untrusted wall-clock; an ordering hint only. Always available, never sufficient alone.                                   |
| `NTP_AUTHENTICATED` | NTS-protected NTP (RFC 8915); cryptographically authenticated wall-clock.                             | Trusted wall-clock when the NTS chain verifies and egress is granted.                                                    |
| `ROUGHTIME`         | Signed single-roundtrip time with a verifiable signature and nonce; misbehaving servers are provable. | Trusted wall-clock; preferred where authenticated bidirectional NTS is unavailable.                                      |
| `TPM_TICK`          | TPM 2.0 monotonic clock + resettable tick counter via signed `TPM2_GetTime` quote.                    | Trusted **monotonicity / skew detection**; not an authoritative wall-clock by itself (TPM clock has no external anchor). |
| `GNSS`              | Time from a GNSS receiver (GPS/Galileo).                                                              | Trusted wall-clock where hardware is present and spoofing controls are recorded; primary candidate for `AIRGAP_HIGH`.    |

Source selection preference (most to least trusted wall-clock anchor):

```text
ROUGHTIME (multi-server quorum)
  > NTP_AUTHENTICATED (NTS)
  > GNSS (with anti-spoof checks)
  > TPM_TICK (ordering/skew only; never sole wall-clock)
  > LOCAL_RTC (untrusted; ordering hint only)
```

`TPM_TICK` is special: it is never a wall-clock authority on its own, but it is
the strongest **skew detector** because its monotonic clock cannot be moved
backward by an attacker who controls the network. The detector (§6) combines a
wall-clock source with `TPM_TICK` monotonicity when a TPM is present.

## 5. Time trust grade

Every timestamp the system emits carries a `TimeTrustGrade`. This is the core
contract of S28: the grade travels with the timestamp into the Evidence Log.

```text
TimeTrustGrade =
  UNTRUSTED_LOCAL
| MONOTONIC_ONLY
| ATTESTED_SINGLE
| ATTESTED_QUORUM
```

Unknown values are rejected by the Evidence Log appender and the TimePosture
loader.

| Grade             | Meaning                                                                                                                                              | Source basis                                                |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| `UNTRUSTED_LOCAL` | Wall-clock from `LOCAL_RTC` with no external verification. Ordering hint only.                                                                       | `LOCAL_RTC` alone.                                          |
| `MONOTONIC_ONLY`  | No verified wall-clock anchor, but monotonic ordering is guaranteed (TPM tick or boot-monotonic). Suitable for sequencing, not for validity windows. | `TPM_TICK` without a verified wall-clock source.            |
| `ATTESTED_SINGLE` | Verified wall-clock from one authenticated source within skew budget.                                                                                | One of `NTP_AUTHENTICATED` / `ROUGHTIME` / `GNSS` verified. |
| `ATTESTED_QUORUM` | Verified wall-clock agreed by ≥2 independent sources within skew budget, ideally cross-checked against `TPM_TICK` monotonicity.                      | ≥2 trusted wall-clock sources agreeing.                     |

Rule (this is the constitutional core of S28, candidate INV-034):

```text
every evidence timestamp declares its TimeTrustGrade
  no receipt may be appended with an absent or unknown grade
  a grade may never be upgraded after the fact
  the grade is computed at emit time and is immutable post-seal (INV-005)
```

This binds directly to S6.3: the receipt envelope's `emitted_at` and `tai64n`
fields already exist; S28 adds the rule that the receipt also records the grade
under which `emitted_at` was written. A receipt stamped `UNTRUSTED_LOCAL` is
still valid evidence — it is honest evidence that the clock was not trusted at
the moment it was written, which is exactly what an auditor needs to know.

## 6. Clock skew detector and skew budget

The `ClockSkewDetector` runs continuously while the host is up. It compares the
selected wall-clock source against every other reachable source and against the
`TPM_TICK` monotonic clock.

```yaml
clock_skew_detector:
  detector_id: "skew_<ULID>"
  reference_source: ROUGHTIME
  cross_check_sources: [NTP_AUTHENTICATED, TPM_TICK]
  monotonic_anchor: TPM_TICK # null if no TPM present
  sample_interval_seconds: 64
  observed_skew_ms: 0 # signed; reference minus local
  monotonic_violation_observed: false # true if clock moved backward
  budget_ref: "skewbudget_secure_default"
  state: WITHIN_BUDGET # see FSM in §7
```

The `SkewBudget` is profile-bound and closed.

```yaml
skew_budget:
  budget_id: "skewbudget_secure_default"
  profile: SECURE_DEFAULT
  soft_skew_ms: 1000 # warn + evidence
  hard_skew_ms: 5000 # block consequential actions
  max_unsynced_seconds: 3600 # max time allowed at MONOTONIC_ONLY/UNTRUSTED_LOCAL
  backward_jump_tolerance_ms: 250 # beyond this a monotonic violation is declared
  require_grade_floor: ATTESTED_SINGLE # min grade for consequential actions on this profile
```

The `±5 s` `emitted_at`-vs-`tai64n` drift signal already named in S6.3 §3 is the
inherited `soft_skew_ms` default for `SECURE_DEFAULT`; S28 makes it a tunable,
profile-bound budget rather than a hard-coded constant. Per-profile budgets are
fixed in §8.

Skew classification:

```text
|observed_skew| <= soft_skew_ms          -> WITHIN_BUDGET
soft_skew_ms < |observed_skew| <= hard    -> SOFT_EXCEEDED (warn + evidence, no block)
|observed_skew| > hard_skew_ms            -> HARD_EXCEEDED (block consequential actions)
monotonic_violation_observed              -> MONOTONIC_VIOLATION (block + force re-attest)
```

## 7. Time posture FSM

The `TimePosture` is the signed, machine-readable truth object the Policy Kernel,
Evidence Log, and renderers read. Its state machine:

```text
COLD_START
  -> read LOCAL_RTC, set grade UNTRUSTED_LOCAL
  -> attempt trusted sources

UNTRUSTED  --(>=1 wall source verified)-->  ATTESTED
UNTRUSTED  --(only TPM tick available)-->   MONOTONIC
ATTESTED   --(skew HARD_EXCEEDED)-->        SKEW_BLOCKED
ATTESTED   --(monotonic violation)-->       SKEW_BLOCKED
ATTESTED   --(all sources lost > max_unsynced)--> DEGRADED
MONOTONIC  --(wall source verified)-->      ATTESTED
SKEW_BLOCKED --(re-attest within budget)--> ATTESTED
DEGRADED   --(profile requires trusted time)--> RECOVERY_REQUIRED
RECOVERY_REQUIRED --(operator enters recovery)--> (handled by S9.1)
```

Forbidden transitions: any path that raises `TimeTrustGrade` without a fresh
verified sample; any direct `SKEW_BLOCKED -> ATTESTED` that skips re-attestation;
any silent exit from `RECOVERY_REQUIRED` without the S9.1 recovery boundary.

`TimePosture` object:

```yaml
time_posture:
  posture_id: "timeposture_<ULID>"
  boot_id: "boot_<ULID>"
  state: ATTESTED
  selected_source: ROUGHTIME
  active_grade: ATTESTED_QUORUM
  agreeing_sources: [ROUGHTIME, NTP_AUTHENTICATED]
  monotonic_anchor_present: true
  skew_detector_ref: "skew_<ULID>"
  budget_ref: "skewbudget_secure_default"
  last_attested_at: "2026-05-29T10:11:12Z"
  last_attested_grade: ATTESTED_QUORUM
  signature_chain: [] # Ed25519 by the S28 time-service key
```

## 8. Security profile gates

| Profile          | Time rule                                                                                                                                                                                                                                                                                   |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `DEV_RELAXED`    | `UNTRUSTED_LOCAL` permitted; skew warnings only; consequential actions allowed with a recorded warning. NTP egress optional.                                                                                                                                                                |
| `SECURE_DEFAULT` | `ATTESTED_SINGLE` required for consequential actions; `soft=1000ms`, `hard=5000ms`, `max_unsynced=3600s`. Falls to warn-and-block when no trusted source.                                                                                                                                   |
| `STIG_ALIGNED`   | `ATTESTED_SINGLE` minimum (quorum preferred); authenticated time required (NTS or Roughtime or GNSS); `hard=2000ms`, `max_unsynced=900s`; loss of trusted time beyond budget forces `RECOVERY_REQUIRED`, never silent local fallback. TPM tick cross-check required where a TPM is present. |
| `AIRGAP_HIGH`    | No live internet NTP. Trusted time from `GNSS` or a signed local stratum-1 mirror or `ROUGHTIME` against an offline-authorized server; `TPM_TICK` mandatory for monotonicity; `hard=2000ms`, `max_unsynced=600s`; loss of trusted time beyond budget forces `RECOVERY_REQUIRED`.            |

Hard denies:

- No consequential typed action may execute when `state = SKEW_BLOCKED` or when
  `active_grade` is below the profile's `require_grade_floor`.
- No AI subject (`AI_NATIVE_SUBJECT`, `AI_AGENT_CAPSULE`) may select, change, or
  override the time source, the `SkewBudget`, or the `TimePosture`. AI may only
  _propose_ a typed `RequestTimeReattest` action; selection is `SYSTEM_SERVICE`
  under policy.
- No `TimeTrustGrade` may be raised retroactively on a sealed receipt (INV-005).
- Under `STIG_ALIGNED`/`AIRGAP_HIGH`, a host may not silently fall back to
  `UNTRUSTED_LOCAL` to keep operating; it degrades to `RECOVERY_REQUIRED`.
- No security profile may be weakened to escape a time gate; weakening the
  profile to bypass `require_grade_floor` is itself a denied action (INV-017).

## 9. Time acquisition under network policy

Reaching `NTP_AUTHENTICATED` or `ROUGHTIME` requires outbound network, which is
default-deny per S8.1. S28 does not bypass it.

```text
time service (SYSTEM_SERVICE subject)
  -> requests OutboundGrant for time egress
  -> S8.1 evaluates: ProtocolFamily UDP/123 (NTS) or Roughtime port
  -> grant is ALLOW_LIST_ONLY, FQDN/IP-pinned, signed
  -> sample acquired, verified, skew computed
  -> TimePosture updated, evidence emitted
```

Rules binding S28 to S8.1:

- Time egress is a named, signed `OutboundGrant` to a pinned set of time servers;
  it is never `ALLOW_INTERNET`.
- An AI subject can never hold the time-egress grant; time acquisition is a
  `SYSTEM_SERVICE` capability (`network.policy.time` class).
- Under `AIRGAP_HIGH`, no live time egress grant is issued; only `GNSS`,
  `TPM_TICK`, and signed local mirror sources are admissible.
- A failed or unreachable time server is evidenced, not silently retried into a
  fallback that pretends trust.

## 10. Recovery with no trusted time

A host can boot with a dead RTC battery, no network, no GNSS, and no prior
attestation. S28 must remain coherent in that case, and recovery must not depend
on the Cognitive Core (INV-001) or on a working clock.

```text
no trusted time available
  -> TimePosture state = UNTRUSTED or MONOTONIC
  -> if profile floor not met for consequential actions:
       block consequential typed actions (read-only operation still allowed)
  -> if profile is STIG_ALIGNED / AIRGAP_HIGH and budget exhausted:
       state = RECOVERY_REQUIRED -> hand to S9.1 recovery boundary
```

Recovery boundary handling (owned by S9.1; S28 binds to it):

- Recovery boot reads and displays the `TimePosture` without the Cognitive Core,
  exactly as it reads the `SecurityProfile`.
- In recovery, the operator may set a **provisional human-asserted time**. This
  produces a receipt graded `UNTRUSTED_LOCAL` with `human_asserted = true`; it
  never produces a trusted grade and never satisfies a profile floor on its own.
- Evidence written during a no-trusted-time window remains valid and append-only;
  its `MONOTONIC_ONLY` / `UNTRUSTED_LOCAL` grade is precisely the audit signal
  that the window occurred. TAI64N monotonic ordering (S6.3) keeps the records
  sequenced even when the wall-clock is unknown.
- Exit from `RECOVERY_REQUIRED` is by reboot into a re-attested posture, per the
  S9.1 exit-by-reboot rule. There is no in-place "trust the clock now" shortcut.

## 11. Operator UX

The operator sees a Time Passport, not chrony logs.

Minimum UI fields:

- current state (Attested / Monotonic only / Untrusted / Skew blocked / Recovery required)
- active trust grade and which sources agreed
- last successful attestation time and source
- observed skew vs budget (soft / hard)
- whether a TPM monotonic anchor is present
- which consequential actions are currently blocked by time, and why
- one-line plain-language reason and the path to restore trust

One-click operator actions:

```text
Re-attest now
Show blocked actions
Use GNSS / local mirror (airgap)
Enter recovery to set provisional time
```

Each action maps to a typed policy decision. The UI is not authority; it cannot
raise the trust grade by itself.

## 12. Evidence records

S28 adds these record types:

```text
TIME_SOURCE_SELECTED
CLOCK_SKEW_DETECTED
TIME_ATTESTATION_VERIFIED
TIME_ATTESTATION_FAILED
TIME_POSTURE_TRANSITION
TIME_MONOTONIC_VIOLATION
CONSEQUENTIAL_ACTION_BLOCKED_TIME_UNTRUSTED
TIME_PROVISIONAL_HUMAN_ASSERTED
```

Minimum fields for `CONSEQUENTIAL_ACTION_BLOCKED_TIME_UNTRUSTED`:

```text
action_id
action_kind
requesting_subject
subject_is_ai
time_posture_id
posture_state
active_grade
required_grade_floor
observed_skew_ms
skew_budget_id
security_profile
block_reason
recommended_remediation
evidence_receipt_id
```

Minimum fields for `TIME_ATTESTATION_VERIFIED`:

```text
attestation_id
time_posture_id
source
grade_assigned
agreeing_sources
monotonic_anchor_present
observed_skew_ms
verified_at
signature_chain_ref
evidence_receipt_id
```

## 13. New invariant candidate

S28 proposes one new constitutional invariant, continuing the Rev.3 sequence
after INV-027 (DEC-R3-010):

```text
INV-034: Every evidence timestamp declares its time-trust grade.
         A receipt is appended only with a known, present TimeTrustGrade;
         the grade is computed at emit time, is immutable post-seal, and may
         never be raised retroactively. An untrusted clock yields honest
         UNTRUSTED_LOCAL / MONOTONIC_ONLY evidence, never a silently trusted
         timestamp.
```

This is the constitutional rule that makes time auditable rather than assumed. It
is mapped in `04_invariants.md` per DEC-R3-010 and is listed in this contract's
return manifest. No inherited invariant is weakened; INV-034 strengthens INV-005
(append-only) and INV-014 (no proof, no completion) by making the _time_ of every
proof carry its own honesty grade.

## 14. Non-goals

- Do not claim AIOS provides a legally traceable time source (UTC traceability /
  qualified timestamping) without an actual accredited time authority.
- Do not let AI subjects select, change, or override the time source, budget, or
  posture.
- Do not silently fall back to an untrusted local clock to keep consequential
  actions flowing under strict profiles.
- Do not require internet NTP under `AIRGAP_HIGH`.
- Do not retroactively upgrade the trust grade of already-sealed evidence.
- Do not replace the Evidence Log's TAI64N ordering; S28 binds to it, it does not
  reinvent it.
- Do not block read-only operation when time is untrusted; only consequential
  typed actions are gated.

## 15. Acceptance criteria

S28 is `REAL` only when:

1. `TrustedTimeSource`, `TimeTrustGrade`, and the `TimePosture` FSM parse and
   reject unknown enum values and unknown posture states.
2. Every emitted evidence receipt carries a known, present `TimeTrustGrade`, and
   the appender rejects a receipt with an absent or unknown grade (INV-034).
3. The `ClockSkewDetector` computes signed skew against ≥1 cross-check source and
   classifies it against a profile-bound `SkewBudget`.
4. Skew beyond the hard budget moves the posture to `SKEW_BLOCKED` and blocks
   consequential typed actions with a `CONSEQUENTIAL_ACTION_BLOCKED_TIME_UNTRUSTED`
   record.
5. A monotonic backward jump beyond tolerance is detected and forces
   re-attestation before any consequential action proceeds.
6. `STIG_ALIGNED` and `AIRGAP_HIGH` require authenticated/attested time and, on
   budget exhaustion, route to `RECOVERY_REQUIRED` rather than untrusted fallback.
7. Recovery can display the active `TimePosture` without the Cognitive Core and
   can record a provisional human-asserted time graded `UNTRUSTED_LOCAL`.
8. Time-source egress is acquired only through a signed S8.1 `OutboundGrant`
   pinned to time servers, and AI subjects can never hold that grant.
9. No `TimeTrustGrade` can be raised on a sealed receipt, and no profile weakening
   can bypass a time gate.
10. The grade assigned to a verified attestation is reproducible from the recorded
    `agreeing_sources` and `observed_skew_ms`.

## 16. See also

- [S3.1 Evidence Log](../../002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/01_evidence_log.md)
- [S6.3 Evidence Receipt Schema](../../002.AI-OS.NET--SPECREV.2/L0_Governance_Evidence_Safety/03_evidence_receipt_schema.md)
- [S8.1 Network Policy](../../002.AI-OS.NET--SPECREV.2/L8_Network_Hardware_Devices/02_network_policy.md)
- [S9.1 Recovery Boundary](../../002.AI-OS.NET--SPECREV.2/L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md)
- [S2.3 Policy Kernel](../../002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S16.1 Security Profile Matrix](../S16_Security_Hardening_Compliance/01_security_profile_matrix.md)
- [Rev.3 Holistic Specification](../00_REV3_HOLISTIC_SPEC.md)
- [Rev.3 Design Decisions](../02_design_decisions.md)
