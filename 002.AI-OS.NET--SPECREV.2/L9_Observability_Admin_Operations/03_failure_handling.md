# Failure Handling and Degradation (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                 |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists, structural contract complete)                                                                                                                                                                                                                         |
| Phase tag      | S14.1                                                                                                                                                                                                                                                                                                                 |
| Layer          | L9 Observability, Admin, Operations                                                                                                                                                                                                                                                                                   |
| Schema package | `aios.failure.v1alpha1`                                                                                                                                                                                                                                                                                               |
| Consumes       | L0 INV-001, INV-007, INV-014; S9.1 Recovery Boundary (`RecoveryEntryReason`); S2.3 Policy Kernel (degraded mode on signature failure); S5.2 Vault Broker (vault unavailability); S8.1 Network Policy (backend degradation); S13.1 Cognitive Core (`DEGRADED` agent state); S10.1 Capability Runtime (adapter failure) |
| Produces       | the closed `FailureClass` taxonomy; the closed `DegradationLevel` FSM; the closed `BehaviorOnFailure` enum; the failure → behavior table; circuit-breaker rules; the runbook lookup contract; ten new evidence record types queued for S3.1 consolidation                                                             |

## §1 Purpose

Every other AIOS spec assumes that "if X breaks, the system does Y." Until this spec, Y was unspecified for most X. The result was that the same failure scenario admitted multiple plausible responses depending on which spec author you asked. That is not a contract.

This sub-spec closes the loop. It defines, in concrete enum-and-table form:

1. **What can go wrong** — a closed `FailureClass` taxonomy that every other spec must classify their failure cases into.
2. **How the system communicates "something is wrong"** — a closed `DegradationLevel` enum that operators, agents, and renderers all read the same way.
3. **What the system does in response** — a closed `BehaviorOnFailure` enum that tells the implementation which of five disciplined responses applies to a given failure.
4. **The mapping** — a closed table from `(FailureClass, layer-context)` to `(BehaviorOnFailure, DegradationLevel, evidence record, recovery path)`.
5. **The discipline** — circuit-breaker rules, anti-cascade rules, anti-suppression rules, and adversarial robustness constraints that prevent the failure-handling machinery from itself becoming a failure surface.

After this spec, every reference to "the system fails closed", "we fall back to a known-good policy bundle", or "this triggers recovery" resolves to a single mechanical concept defined here.

## §2 Scope

This spec **defines**:

1. The closed `FailureClass` taxonomy (§3.1) — the universe of failure modes AIOS recognizes.
2. The closed `DegradationLevel` FSM (§3.2) — the system's overall health state.
3. The closed `BehaviorOnFailure` enum (§3.3) — the response choices.
4. The failure → behavior mapping table (§4).
5. The DegradationLevel state machine (§5).
6. The circuit-breaker discipline (§6).
7. The runbook lookup contract (§7).
8. The anti-cascade rules (§8).
9. The adversarial robustness profile (§9).
10. The telemetry contract (§10).
11. Three worked examples (§11).
12. Ten evidence record types queued for S3.1 consolidation (§12).
13. The acceptance criteria (§13).

This spec **does not** define:

- The runbook **content** itself — the operator-facing instructions per failure class are operator documentation, not contract surface. This spec defines the _lookup path_ and the _naming convention_; the runbook text lives in `/aios/system/runbooks/<failure_class>/...` AIOS-FS objects (S4.1).
- The recovery boot pipeline — owned by S9.1 (`RecoveryEntryReason` is consumed here, not defined here).
- The verification grammar primitives — owned by S2.4 (this spec consumes `VerificationStatus` and the probe-error vs verification-fail distinction).
- The evidence log shape — owned by S3.1 (this spec queues new record types; the storage and chain mechanics are S3.1's contract).
- The vault, policy, identity bundle internal verification — owned by L4; this spec only consumes the binary outcome ("signature failed") and dictates the system response.
- Per-adapter health-probe semantics — owned by S10.1 / S1.1 manifests.

This spec is the **contract surface** that every other spec references when it says "fail closed", "degrade to known good", or "drop into recovery".

## §3 Vocabulary (closed enums)

All vocabularies in this section are closed. Adding a value is a versioned spec change. Bundle load fails on unknown values.

### §3.1 `FailureClass`

The top-level failure taxonomy. Every detected failure in AIOS is classified into exactly one of these values. There is no `OTHER`, no `UNKNOWN_FAILURE`, no open-ended "free-form failure" string. A failure that does not fit one of these classes is an indication that this spec is incomplete and must be amended — not that the runtime should invent a new class.

```proto
syntax = "proto3";
package aios.failure.v1alpha1;

enum FailureClass {
  FAILURE_CLASS_UNSPECIFIED            = 0;
  COMPONENT_UNAVAILABLE                = 1;   // a layer/service is offline
  COMPONENT_DEGRADED                   = 2;   // running but reduced capability
  BUNDLE_SIGNATURE_FAILURE             = 3;   // invariant/policy/identity bundle Ed25519 verification failed
  HARDWARE_FAILURE                     = 4;   // disk failure, GPU disconnect, NIC dead
  VAULT_UNAVAILABLE                    = 5;   // vault broker cannot serve requests
  AI_PROVIDER_UNAVAILABLE              = 6;   // local model not running OR external provider returns errors
  POLICY_DECISION_TIMEOUT              = 7;   // policy kernel took too long
  VERIFICATION_TIMEOUT                 = 8;   // verification engine probe timed out
  ADAPTER_FAILURE                      = 9;   // L3 adapter panicked or refused
  NETWORK_PARTITION                    = 10;  // host lost network
  TAMPER_DETECTED                      = 11;  // evidence chain or bundle tamper observed
  RESOURCE_EXHAUSTION                  = 12;  // disk full, OOM, queue depth exceeded
  TIME_DRIFT                           = 13;  // clock skew exceeds tolerance
  BACKEND_VERSION_MISMATCH             = 14;  // host kernel/firmware doesn't match AIOS expectation
  RECOVERY_OPERATOR_UNAVAILABLE        = 15;  // at recovery time, operator credentials cannot be verified
}
```

| Value                           | One-line statement                                                                                                                                       |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `COMPONENT_UNAVAILABLE`         | A named AIOS service (policy kernel, identity, capability runtime, verification engine, evidence log, AIOS-FS daemon, ...) does not respond to requests. |
| `COMPONENT_DEGRADED`            | A named AIOS service responds but is missing capability (e.g. policy kernel running on the previous bundle because the new one failed to load).          |
| `BUNDLE_SIGNATURE_FAILURE`      | An invariant, policy, identity, capability, or sandbox bundle was presented and its Ed25519 signature did not verify against the AIOS root signing key.  |
| `HARDWARE_FAILURE`              | A hardware resource transitioned from healthy to unusable: disk I/O errors, GPU disconnect, NIC down, TPM unavailable, sensor read failure.              |
| `VAULT_UNAVAILABLE`             | The L4.2 vault broker did not respond, or responded that it cannot fulfill requests (sealed, locked, TPM unavailable post-boot).                         |
| `AI_PROVIDER_UNAVAILABLE`       | The local model runtime is not reachable, OR an external provider returned unrecoverable errors after retry.                                             |
| `POLICY_DECISION_TIMEOUT`       | The policy kernel did not return a decision within its budget for an action.                                                                             |
| `VERIFICATION_TIMEOUT`          | A verification primitive or composition timed out per S2.4 §6.3.                                                                                         |
| `ADAPTER_FAILURE`               | An L3 capability adapter panicked, returned an unparseable error, or refused with internal-error class.                                                  |
| `NETWORK_PARTITION`             | The host's network plane is down or split; outbound and/or inbound connectivity has gone away.                                                           |
| `TAMPER_DETECTED`               | The evidence chain or a signed bundle was found inconsistent with its hash/signature post-load.                                                          |
| `RESOURCE_EXHAUSTION`           | A bounded resource (disk, RAM, queue depth, file descriptors) is exhausted.                                                                              |
| `TIME_DRIFT`                    | Wall-clock skew exceeds the tolerance required for signature TTLs and approval expiry.                                                                   |
| `BACKEND_VERSION_MISMATCH`      | The host's kernel / firmware / AIOS substrate version does not match the AIOS-FS state's required version (e.g. older kernel on newer rootfs).           |
| `RECOVERY_OPERATOR_UNAVAILABLE` | At recovery time, the operator credentials required by S9.1 cannot be verified (lost token, hardware key absent, identity service in degraded mode).     |

The closed enum has 15 values plus UNSPECIFIED. The list is a contract: no failure is allowed to be classified outside it. **A new failure mode** observed in production triggers a versioned spec amendment to add a class — never an `OTHER` bucket and never an inline string.

#### §3.1.1 Class is the _system's_ perspective

`FailureClass` is the system-level perspective. An adapter that crashes is `ADAPTER_FAILURE` regardless of whether the underlying cause is a kernel bug or a misconfigured probe. The adapter-level diagnostic (panic stack, exit code) lives inside the evidence record's payload; the _class_ is uniform. This is intentional — it keeps the cross-layer behavior table tractable and prevents the open-ended explosion that would happen if every adapter authored its own failure class.

#### §3.1.2 Classifying compound failures

When a single root cause manifests as several observed failures (e.g. a network partition makes the policy kernel time out, the vault unreachable, and the AI provider unreachable simultaneously), the runtime emits **one record per observation**, each with its own class. The runtime does not collapse them. The correlation engine in L9 telemetry stitches them into a single incident at query time, but the on-write record is per-symptom. This is so that any one record being lost (or the storage tier being trimmed) does not lose other records of the same incident.

### §3.2 `DegradationLevel`

The system-wide health state. Every AIOS host is, at any wall-clock instant, in exactly one degradation level. The level is observable through L9 telemetry, the renderer trust bar (per S7.2 INV-020), and the policy kernel decision context.

```proto
enum DegradationLevel {
  DEGRADATION_LEVEL_UNSPECIFIED = 0;
  NORMAL                        = 1;   // full operation; all components green
  DEGRADED_SOFT                 = 2;   // non-critical functionality reduced; system works
  DEGRADED_HARD                 = 3;   // critical functionality reduced; only essential ops possible
  READ_ONLY                     = 4;   // accepts queries; rejects mutations
  RECOVERY_PENDING              = 5;   // must reboot into recovery
  HALTED                        = 6;   // system refuses any operation; awaiting operator
}
```

| Value              | Meaning                                                                                                                                                                                                                              |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `NORMAL`           | All declared components healthy; no active degradation. Default state.                                                                                                                                                               |
| `DEGRADED_SOFT`    | A non-critical component is degraded or unavailable. The system continues to accept actions, but some optional functionality is unavailable. Example: external AI provider 5xx, GPU disconnect on a non-rendering host.              |
| `DEGRADED_HARD`    | A critical component is degraded or unavailable. The system continues to accept actions but with significant restriction. Example: vault unavailable (no secret-bearing actions possible), policy kernel running on previous bundle. |
| `READ_ONLY`        | The system accepts queries (List/Get/Verify on read paths) but rejects all mutations. Mutation-attempt evidence is still emitted. Used when AIOS-FS WAL is suspect or disk is full.                                                  |
| `RECOVERY_PENDING` | The system has determined that continued normal operation is not safe. Existing in-flight actions complete or fail; new actions are rejected. The system is awaiting a reboot into the S9.1 recovery boot path.                      |
| `HALTED`           | The system refuses every operation including reads, except evidence emission and the trust-bar status surface. Used for `TAMPER_DETECTED` and `BACKEND_VERSION_MISMATCH`. Exit requires a recovery boot.                             |

There is no fourth-and-a-half level. `MAINTENANCE`, `SAFE`, `LIMITED` are _not_ AIOS levels — any spec or implementation that introduces a value beyond these six is in violation.

#### §3.2.1 Level is monotone-fragile

Levels move toward more-restrictive values (`NORMAL → DEGRADED_SOFT → DEGRADED_HARD → READ_ONLY → RECOVERY_PENDING → HALTED`) on failure events. Levels move toward less-restrictive values **only** when the underlying cause is observed cleared — and only as far as `NORMAL` from `DEGRADED_SOFT` and from `DEGRADED_HARD`. There is no automatic recovery from `READ_ONLY`, `RECOVERY_PENDING`, or `HALTED`; those require an explicit human action (cleanup, recovery boot, reinstall).

#### §3.2.2 Level is read by every renderer

Per L0 INV-020 (trust indicators always visible), the active `DegradationLevel` is rendered in the trust bar at all times. The renderer reads the level from the L9 health surface, never from a guess.

### §3.3 `BehaviorOnFailure`

The closed set of disciplined responses. Every entry in the failure → behavior table (§4) selects exactly one of these. There is no compound behavior, no "try X then Y then Z" except as encoded by `AUTOMATIC_RETRY` (which is a single behavior with bounded budget).

```proto
enum BehaviorOnFailure {
  BEHAVIOR_ON_FAILURE_UNSPECIFIED = 0;
  FAIL_CLOSED                     = 1;   // operation rejected; evidence emitted; no fallback
  DEGRADE_TO_KNOWN_GOOD           = 2;   // fall back to a known-good prior state
  DEFER_TO_RECOVERY               = 3;   // drop into recovery for operator decision
  AUTOMATIC_RETRY                 = 4;   // bounded retry; if exhausted, FAIL_CLOSED
  HUMAN_DECISION_REQUIRED         = 5;   // pause; require operator approval
}
```

| Value                     | Meaning                                                                                                                                                                                                                                                                                                                   |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `FAIL_CLOSED`             | The operation under attempt is rejected. Evidence is emitted. No alternative is tried. The default behavior in the absence of explicit table guidance. Aligns with the AIOS rule that an absent or ambiguous policy denies.                                                                                               |
| `DEGRADE_TO_KNOWN_GOOD`   | The system falls back to a known-good prior state and continues operation. Example: policy bundle update fails signature verification → continue using the previously active bundle. This behavior is allowed only when the prior state is itself well-defined and signed.                                                |
| `DEFER_TO_RECOVERY`       | The system enters `RECOVERY_PENDING` (or `HALTED` for catastrophic classes). The operator is expected to reboot into recovery to repair. No further automated action is attempted.                                                                                                                                        |
| `AUTOMATIC_RETRY`         | The operation is retried with bounded exponential backoff. Each FailureClass that uses this behavior carries an explicit retry budget (count and total elapsed time). When the budget is exhausted, behavior collapses to `FAIL_CLOSED`. Retry budgets are constants per (FailureClass, layer) pair and are listed in §6. |
| `HUMAN_DECISION_REQUIRED` | The operation is paused and an approval request is emitted to the operator. Until the operator decides (allow / deny / defer), the operation is not retried automatically. Used when the system has enough information to know that automated decision is unsafe but not enough to know that recovery is required.        |

`FAIL_CLOSED` is the **default**. If a failure occurs in a context not covered by §4, the system fails closed. This is INV-008 (default deny) extended into the failure-handling domain.

## §4 Failure → behavior table

This is the heart of the spec. For each `FailureClass` and the layer in which it can be observed, the table specifies the `BehaviorOnFailure`, the `DegradationLevel` it triggers, the queued evidence record (§12), and the recovery path. The table is closed: an entry that is not present means the default `FAIL_CLOSED` + nearest matching degradation applies, and the runtime emits a `FAILURE_OBSERVED` evidence record naming the gap so the spec can be amended.

### §4.1 The mapping

| #   | FailureClass                  | Layer-context                                   | Behavior                                                        | DegradationLevel               | Evidence record                                              | Recovery path                                            |
| --- | ----------------------------- | ----------------------------------------------- | --------------------------------------------------------------- | ------------------------------ | ------------------------------------------------------------ | -------------------------------------------------------- |
| 1   | BUNDLE_SIGNATURE_FAILURE      | L0 invariant bundle                             | FAIL_CLOSED + degrade to INV-001 + INV-002 only                 | DEGRADED_HARD                  | `INVARIANT_BUNDLE_REJECTED` (FOREVER)                        | Recovery + HUMAN_USER + replace bundle                   |
| 2   | BUNDLE_SIGNATURE_FAILURE      | L4.1 policy bundle                              | DEGRADE_TO_KNOWN_GOOD; if no known-good then FAIL_CLOSED        | DEGRADED_SOFT or DEGRADED_HARD | `POLICY_BUNDLE_REJECTED` (FOREVER)                           | Operator submits correctly-signed bundle; or recovery    |
| 3   | BUNDLE_SIGNATURE_FAILURE      | L4.3 identity bundle                            | DEGRADE_TO_KNOWN_GOOD                                           | DEGRADED_HARD                  | `IDENTITY_BUNDLE_REJECTED` (FOREVER)                         | Operator submits correctly-signed bundle; or recovery    |
| 4   | BUNDLE_SIGNATURE_FAILURE      | L1.1 capability bundle                          | DEGRADE_TO_KNOWN_GOOD                                           | DEGRADED_SOFT                  | `CAPABILITY_BUNDLE_REJECTED` (FOREVER)                       | Operator submits correctly-signed bundle                 |
| 5   | BUNDLE_SIGNATURE_FAILURE      | L6.3 sandbox bundle                             | DEGRADE_TO_KNOWN_GOOD                                           | DEGRADED_SOFT                  | `SANDBOX_BUNDLE_REJECTED` (FOREVER)                          | Operator submits correctly-signed bundle                 |
| 6   | COMPONENT_UNAVAILABLE         | L4.1 policy kernel                              | FAIL_CLOSED on every action                                     | DEGRADED_HARD                  | `FAILURE_OBSERVED` (STANDARD_24M)                            | Operator restart; if 3x within 5 min → DEFER_TO_RECOVERY |
| 7   | COMPONENT_UNAVAILABLE         | L4.3 identity service                           | FAIL_CLOSED on subject-resolving operations                     | DEGRADED_HARD                  | `FAILURE_OBSERVED` (STANDARD_24M)                            | Operator restart; cached subjects continue to operate    |
| 8   | COMPONENT_UNAVAILABLE         | L9.1 evidence log                               | FAIL_CLOSED on every action (no evidence = no action)           | RECOVERY_PENDING               | `FAILURE_OBSERVED` (STANDARD_24M, in-memory)                 | Recovery — INV-014 forbids action without evidence       |
| 9   | COMPONENT_UNAVAILABLE         | L9.2 verification engine                        | FAIL_CLOSED on every state-changing action                      | DEGRADED_HARD                  | `FAILURE_OBSERVED` (STANDARD_24M)                            | Operator restart                                         |
| 10  | COMPONENT_UNAVAILABLE         | L3 capability runtime                           | FAIL_CLOSED on every action                                     | DEGRADED_HARD                  | `FAILURE_OBSERVED` (STANDARD_24M)                            | Operator restart                                         |
| 11  | COMPONENT_UNAVAILABLE         | L2 AIOS-FS daemon                               | DEFER_TO_RECOVERY                                               | RECOVERY_PENDING               | `FAILURE_OBSERVED` (STANDARD_24M)                            | Recovery                                                 |
| 12  | COMPONENT_DEGRADED            | L4.1 policy kernel (running on previous bundle) | Continue with previous bundle                                   | DEGRADED_SOFT                  | `DEGRADATION_LEVEL_TRANSITIONED` (STANDARD_24M)              | Operator submits correctly-signed bundle                 |
| 13  | COMPONENT_DEGRADED            | L4.3 identity service (degraded mode)           | Only `_system` subjects available                               | DEGRADED_HARD                  | `DEGRADATION_LEVEL_TRANSITIONED` (STANDARD_24M)              | Recovery for non-`_system` subjects                      |
| 14  | VAULT_UNAVAILABLE             | L4.2 vault broker                               | FAIL_CLOSED on all vault-requiring operations                   | DEGRADED_HARD                  | `FAILURE_OBSERVED` (EXTENDED_60M, vault tag)                 | Operator repair / TPM hardware repair                    |
| 15  | AI_PROVIDER_UNAVAILABLE       | L5 cognitive core, local model dead             | DEGRADE_TO_KNOWN_GOOD (no-LLM direct path)                      | DEGRADED_SOFT                  | `FAILURE_OBSERVED` (STANDARD_24M)                            | Operator restart provider                                |
| 16  | AI_PROVIDER_UNAVAILABLE       | L5 cognitive core, external provider 5xx        | AUTOMATIC_RETRY (3x with backoff) → FAIL_CLOSED                 | DEGRADED_SOFT                  | `FAILURE_OBSERVED` (EXTENDED_60M)                            | Operator review                                          |
| 17  | POLICY_DECISION_TIMEOUT       | L4.1 policy kernel                              | FAIL_CLOSED                                                     | DEGRADED_SOFT                  | `FAILURE_OBSERVED` (EXTENDED_60M)                            | Operator review                                          |
| 18  | VERIFICATION_TIMEOUT          | L9.2 verification engine                        | mark `VerificationStatus = TIMEOUT`; FAIL_CLOSED action         | DEGRADED_SOFT                  | `FAILURE_OBSERVED` (EXTENDED_60M)                            | Operator review                                          |
| 19  | ADAPTER_FAILURE               | L3 adapter panic                                | FAIL_CLOSED action; mark adapter DEGRADED in S10.1              | DEGRADED_SOFT                  | `FAILURE_OBSERVED` (EXTENDED_60M)                            | Operator review or auto-restart per S10.1                |
| 20  | NETWORK_PARTITION             | L8 network plane                                | DEGRADE_TO_KNOWN_GOOD (offline mode); LAN exposures auto-revoke | DEGRADED_SOFT                  | `FAILURE_OBSERVED` (STANDARD_24M)                            | Auto-recover when network returns                        |
| 21  | HARDWARE_FAILURE              | L8.2 GPU disconnect                             | revoke GPU bindings; APP_SURFACE falls back to software path    | DEGRADED_SOFT                  | `FAILURE_OBSERVED` (EXTENDED_60M)                            | Hardware reattach                                        |
| 22  | HARDWARE_FAILURE              | L4.2 TPM unavailable post-boot                  | FAIL_CLOSED on vault unseal                                     | DEGRADED_HARD                  | `FAILURE_OBSERVED` (FOREVER)                                 | Hardware repair / recovery                               |
| 23  | HARDWARE_FAILURE              | L2 disk I/O errors on /aios                     | DEFER_TO_RECOVERY                                               | READ_ONLY → RECOVERY_PENDING   | `FAILURE_OBSERVED` (FOREVER)                                 | Recovery                                                 |
| 24  | RESOURCE_EXHAUSTION           | L2 disk full on /aios                           | FAIL_CLOSED on writes; reads continue                           | READ_ONLY                      | `FAILURE_OBSERVED` (EXTENDED_60M)                            | Operator cleanup                                         |
| 25  | RESOURCE_EXHAUSTION           | L4.1 OOM on policy kernel                       | restart with backoff; if 3x in 5 min then DEFER_TO_RECOVERY     | DEGRADED_HARD                  | `COMPONENT_RESTARTED` / `COMPONENT_RESTART_BUDGET_EXHAUSTED` | Resource increase / recovery                             |
| 26  | RESOURCE_EXHAUSTION           | L3 capability runtime queue depth exceeded      | backpressure (reject new); existing continue                    | DEGRADED_SOFT                  | `CIRCUIT_BREAKER_OPENED` (EXTENDED_60M)                      | Wait or scale                                            |
| 27  | TAMPER_DETECTED               | L9.1 evidence chain hash mismatch               | DEFER_TO_RECOVERY                                               | RECOVERY_PENDING → HALTED      | `TAMPER_DETECTED` (FOREVER)                                  | Recovery + forensic analysis                             |
| 28  | TAMPER_DETECTED               | L4.1 policy bundle tampered post-load           | DEFER_TO_RECOVERY                                               | RECOVERY_PENDING               | `TAMPER_DETECTED` (FOREVER)                                  | Recovery                                                 |
| 29  | TIME_DRIFT                    | L0 wall-clock skew > tolerance                  | FAIL_CLOSED on all time-bound ops (TTLs, signatures)            | DEGRADED_HARD                  | `TIME_DRIFT_DETECTED` (EXTENDED_60M)                         | NTP repair                                               |
| 30  | BACKEND_VERSION_MISMATCH      | L1 boot-time kernel/AIOS-FS version mismatch    | FAIL_CLOSED at boot; halt before /aios mount                    | HALTED                         | `BACKEND_VERSION_MISMATCH` (FOREVER)                         | Recovery + kernel/firmware upgrade                       |
| 31  | RECOVERY_OPERATOR_UNAVAILABLE | L1 recovery boot                                | HALTED; pause at `STAGE_RECOVERY_SHELL_READY` (S9.1)            | HALTED                         | `FAILURE_OBSERVED` (FOREVER)                                 | Manual operator intervention or factory reset            |

That is 31 closed scenario entries spanning 11 of the 15 `FailureClass` values. The four classes that do not appear in the table by index — `COMPONENT_UNAVAILABLE`, `RESOURCE_EXHAUSTION`, `HARDWARE_FAILURE`, `BUNDLE_SIGNATURE_FAILURE` — appear repeatedly across multiple layer-contexts above. Two classes (`COMPONENT_DEGRADED`, `BUNDLE_SIGNATURE_FAILURE`) cover the entire bundle catalog (invariant, policy, identity, capability, sandbox).

#### §4.2 Entries that are not in the table

If a runtime observation does not match any entry in §4.1, the runtime:

1. emits a `FAILURE_OBSERVED` record with `failure_class` set and `coverage = "uncovered"`,
2. applies `FAIL_CLOSED` as the default behavior,
3. transitions to the most-restrictive applicable `DegradationLevel` (default `DEGRADED_SOFT`; `DEGRADED_HARD` if the failure is in a critical component per §3.2),
4. queues a spec amendment item.

This is intentionally noisy: an uncovered failure is itself a defect in this spec. The runtime must surface it, never paper over it.

#### §4.3 The table is normative for evidence

The "Evidence record" column is the _minimum_ required record. Implementations may emit additional records (e.g. correlation, narrative annotations) but must emit at least the listed one with the listed retention class.

#### §4.4 The table is normative for level

The "DegradationLevel" column is the _floor_ — the minimum level the system enters on this failure. The system may be in a higher (more restrictive) level for unrelated reasons; an `OR` semantic applies (the level is the maximum of all currently-active level requirements).

#### §4.5 Nuances per row

A few entries deserve a note on their disciplined meaning.

- **Row 1 — invariant bundle signature failure → INV-001 + INV-002 only.** When the L0 invariant bundle fails verification, AIOS does not refuse to boot; it boots with only INV-001 (recovery independent of L5) and INV-002 (AI proposes, never executes) active. All other invariants are _implicit_ in the runtime code. The reason: a system that refuses to boot because the constitution failed verification is not recoverable. A system that boots with the two minimum constitutional rules can still serve recovery operations. This is per L0 §2 I3.

- **Row 2 — policy bundle DEGRADE_TO_KNOWN_GOOD.** The policy kernel maintains the previously-loaded signed bundle in memory. On a failed bundle update, the kernel continues using the previous bundle and emits `POLICY_BUNDLE_REJECTED` with FOREVER retention. This is not a "soft accept" — the rejected bundle is rejected forever for that signature; only a different (correctly signed) bundle replaces it. If there is no previous bundle (first boot, no known-good), the kernel falls to FAIL_CLOSED on every action — DEGRADED_HARD.

- **Row 8 — evidence log unavailable → RECOVERY_PENDING.** This is the strictest entry in the table because INV-014 forbids any action without evidence. Without the evidence log accepting writes, the only safe behavior is to halt mutations entirely. The runtime keeps a small in-memory ring buffer of `FAILURE_OBSERVED` records that flush on log recovery, but it does not allow any state-changing action to proceed.

- **Row 16 — external AI provider AUTOMATIC_RETRY.** External provider 5xx is the only entry in the table that uses AUTOMATIC_RETRY. The retry budget is **3 attempts** with exponential backoff (1s, 2s, 4s), capped at **8 seconds total**. Beyond the budget, behavior collapses to FAIL_CLOSED with a `FAILURE_OBSERVED` record at EXTENDED_60M retention. The retry is per-call, not per-subject.

- **Row 25 — OOM on policy kernel.** A bounded restart budget applies (3 restarts within 5 minutes). If the budget is exhausted, the runtime escalates to DEFER_TO_RECOVERY. The 3-in-5 budget is repeated for several other components (identity, capability runtime); see §6 for the full list.

- **Row 27 — TAMPER_DETECTED on evidence chain → HALTED.** Evidence chain tampering is the most serious failure class. The runtime escalates DegradationLevel through RECOVERY_PENDING and continues to HALTED if the tamper persists across a single re-verify pass. The reason: an evidence chain that has been mutated is unsafe to operate on, and operating on it is a worse outcome than halting.

- **Row 31 — RECOVERY_OPERATOR_UNAVAILABLE.** The system reaches the recovery boot path's `STAGE_RECOVERY_SHELL_READY` (S9.1) and, having no operator credential to verify, pauses indefinitely. There is no automatic factory reset. This is intentional: an automated factory reset triggered by missing credentials is a denial-of-service attack vector.

## §5 DegradationLevel state machine

The `DegradationLevel` FSM defines the closed transitions. No transition outside this table is permitted.

### §5.1 Closed transitions table

```text
NORMAL              ─[failure of class C row in §4.1 with level=DEGRADED_SOFT]─→ DEGRADED_SOFT
NORMAL              ─[failure of class C row in §4.1 with level=DEGRADED_HARD]─→ DEGRADED_HARD
NORMAL              ─[failure of class C row in §4.1 with level=READ_ONLY]─→ READ_ONLY
NORMAL              ─[failure of class C row in §4.1 with level=RECOVERY_PENDING]─→ RECOVERY_PENDING
NORMAL              ─[TAMPER_DETECTED or BACKEND_VERSION_MISMATCH catastrophic]─→ HALTED

DEGRADED_SOFT       ─[underlying class cleared, all soft causes resolved]─→ NORMAL
DEGRADED_SOFT       ─[additional failure escalates]─→ DEGRADED_HARD | READ_ONLY | RECOVERY_PENDING | HALTED

DEGRADED_HARD       ─[underlying class cleared, no soft causes remain]─→ NORMAL
DEGRADED_HARD       ─[underlying class cleared, soft causes remain]─→ DEGRADED_SOFT
DEGRADED_HARD       ─[escalation per row in §4.1]─→ READ_ONLY | RECOVERY_PENDING | HALTED

READ_ONLY           ─[underlying cause cleared by operator action only]─→ DEGRADED_SOFT or DEGRADED_HARD
READ_ONLY           ─[escalation]─→ RECOVERY_PENDING | HALTED

RECOVERY_PENDING    ─[reboot into recovery (S9.1)]─→ [out of FSM; recovery boot path takes over]
RECOVERY_PENDING    ─[escalation]─→ HALTED

HALTED              ─[reboot into recovery (S9.1)]─→ [out of FSM]
HALTED              ─[any other]─→ HALTED  (terminal until reboot)
```

### §5.2 Transition discipline

- **Auto-relax allowed only NORMAL ← {DEGRADED_SOFT, DEGRADED_HARD}.** When the underlying failure causes are observed cleared, the system can return to NORMAL (or to DEGRADED_SOFT from DEGRADED_HARD if some soft causes remain). This is a closed-loop transition driven by health probes.
- **No auto-relax from {READ_ONLY, RECOVERY_PENDING, HALTED}.** These three levels require explicit human action. READ_ONLY waits for the operator to clear the disk-full or WAL-suspect condition. RECOVERY_PENDING and HALTED both exit by reboot into the S9.1 recovery boot path.
- **Escalation is allowed from any level toward more-restrictive.** A SOFT host that observes a critical failure can transition straight to HALTED if the table calls for it.
- **Level transitions are evidence.** Every transition emits `DEGRADATION_LEVEL_TRANSITIONED` with old + new level, the triggering FailureClass, and the layer-context.
- **Level is bounded by uptime.** A host that has been HALTED does not silently transition out of HALTED on its own. Only a recovery boot clears HALTED, and the next boot starts at NORMAL (assuming the underlying issue is resolved) or transitions to HALTED again on the same observation.

### §5.3 Level visibility

The active level is visible to:

- **Renderers** via the L9 health surface (per L0 INV-020).
- **Policy kernel** as part of the decision context — bundles can be authored with rules conditional on level (e.g. "deny non-emergency mutations when level >= DEGRADED*HARD"). Note: this is \_additional* policy on top of the baseline behavior table — it does not replace §4.
- **Operators** via the trust bar and the L9 admin CLI.
- **L5 cognitive core** as a session enrichment field. Agents asked to plan when level >= DEGRADED_HARD must surface this in their plan rationale.
- **Telemetry** as the gauge `degradation_level_active{level}`.

The level is **never** writable by an L5 subject. Only L0-rule-driven runtime code mutates it. Agents that attempt to manipulate the level field are blocked at S2.3 hard-deny `LevelManipulationByAi`.

## §6 Circuit-breaker discipline

A failed component must not retry indefinitely. A failed component must not amplify its failure into other components.

### §6.1 Per-component restart budgets

| Component             | Budget               | Behavior on exhaustion              |
| --------------------- | -------------------- | ----------------------------------- |
| Policy kernel         | 3 restarts in 5 min  | DEFER_TO_RECOVERY                   |
| Identity service      | 3 restarts in 5 min  | DEGRADED_HARD; cached subjects only |
| Capability runtime    | 3 restarts in 5 min  | DEFER_TO_RECOVERY                   |
| Verification engine   | 3 restarts in 5 min  | DEGRADED_HARD                       |
| Evidence log writer   | 3 restarts in 5 min  | DEFER_TO_RECOVERY                   |
| Vault broker          | 3 restarts in 5 min  | DEGRADED_HARD                       |
| AIOS-FS daemon        | 3 restarts in 5 min  | DEFER_TO_RECOVERY                   |
| L5 local model runner | 5 restarts in 10 min | DEGRADED_SOFT (AI provider unavail) |
| Adapter (per-id)      | 5 restarts in 10 min | adapter marked DEGRADED in S10.1    |
| Renderer (per-id)     | 5 restarts in 10 min | renderer surface failure path       |

The 3-in-5 budget is the canonical "tight loop" budget; the 5-in-10 budget is the canonical "soft recovery" budget. The runtime emits `COMPONENT_RESTARTED` per restart and `COMPONENT_RESTART_BUDGET_EXHAUSTED` on exhaustion (FOREVER retention).

### §6.2 Circuit-breaker for outbound calls

Outbound calls (external AI provider, network-based health probes, telemetry exporters) follow a per-target circuit breaker:

- **Closed state.** Calls go through normally.
- **Half-open state.** After N consecutive failures, the breaker opens — new calls fail fast for a cool-down. After the cool-down, one probe call is allowed; on success, breaker closes; on failure, breaker re-opens with double cool-down.
- **Open state.** All calls return immediately with `BEHAVIOR_ON_FAILURE = FAIL_CLOSED` and `failure_class = AI_PROVIDER_UNAVAILABLE` (or `NETWORK_PARTITION`).

| Target type          | Open after | Initial cool-down | Max cool-down |
| -------------------- | ---------- | ----------------- | ------------- |
| External AI provider | 3 failures | 30 s              | 5 min         |
| Telemetry exporter   | 5 failures | 1 min             | 15 min        |
| Network-based probe  | 3 failures | 10 s              | 2 min         |

The circuit-breaker emits `CIRCUIT_BREAKER_OPENED` (EXTENDED_60M) on open, `CIRCUIT_BREAKER_CLOSED` (STANDARD_24M) on close.

### §6.3 No infinite retry

Every retry budget in this spec is bounded. There is no behavior that triggers unbounded retries. AUTOMATIC_RETRY is bounded per §4.5 row 16 (3 attempts, 8 seconds total). Restart budgets are bounded per §6.1. Circuit-breaker cool-downs are bounded per §6.2. A failure that persists past these bounds escalates to a more-restrictive behavior — either FAIL_CLOSED, DEFER_TO_RECOVERY, or HALTED.

### §6.4 Recovery loop detection

A recovery boot triggered by automated escalation is itself a failure of the previous normal-mode session. If recovery is entered N times within M minutes for the same `RecoveryEntryReason` (S9.1), the system halts and emits `RECOVERY_LOOP_DETECTED` (FOREVER) at `STAGE_RECOVERY_SHELL_READY`. Default thresholds: **N = 3 within M = 60 minutes** for the same entry reason. The operator must intervene; the system does not loop infinitely between normal and recovery.

## §7 Runbook lookup

For each `FailureClass`, the operator-facing runbook (step-by-step instructions) lives at a canonical AIOS-FS path:

```text
/aios/system/runbooks/<failure_class_lower>/index.md
/aios/system/runbooks/<failure_class_lower>/<sub_topic>.md
```

Where `<failure_class_lower>` is the FailureClass enum name lowercased with `_` separators preserved, e.g. `bundle_signature_failure`, `vault_unavailable`, `tamper_detected`.

### §7.1 Lookup contract

- The runbook lookup path is **canonical and per-failure-class**. The runtime's failure-emission code does not embed the runbook content; it emits a reference (`runbook_path`) into the evidence record.
- The runbook content itself is **out of scope** for this spec — it is operator documentation, not contract.
- The runbook path resolves through the active S4.1 namespace catalog. Recovery-mode reads use the S9.1 read-only forensic attach path.
- A missing runbook is **not** a failure — it is a documentation gap. The runtime emits `FAILURE_OBSERVED` with `runbook_present = false` so the operator can see which classes lack runbooks.

### §7.2 Runbook envelope

Runbooks themselves carry a small machine-readable header — the runtime reads only this; the body is human-only.

```yaml
# /aios/system/runbooks/<failure_class>/index.md (header only)
schema_version: aios.runbook.v1alpha1
failure_class: VAULT_UNAVAILABLE
applies_to_layers: [L4]
severity: critical
expected_recovery_path: operator_repair
estimated_resolution_time: hours
last_reviewed_at: "2026-01-15"
```

The runtime ignores the body. The L9 admin CLI surfaces the body to operators when an alert fires.

## §8 Anti-cascade discipline

A failed component does not propagate its failure beyond its scope.

### §8.1 Acyclic degradation propagation

Degradation propagates **downward** in the layer model (per INV-007): a failure in L4 (policy kernel) can degrade L3 (capability runtime) because L3 depends on L4. A failure in L3 does **not** degrade L4 — the policy kernel does not become unhealthy because adapters are unhealthy. Propagation is therefore acyclic.

### §8.2 Per-layer scope

A failure in one component restricts exactly the operations that depend on that component. Specifically:

- A failure in the **policy kernel** restricts new actions (no decisions). It does **not** invalidate already-decided actions in flight; those continue with their existing decision.
- A failure in the **vault broker** restricts only operations that need vault material; non-vault operations continue.
- A failure in the **L5 cognitive core** restricts only AI-mediated paths; direct (no-LLM) paths continue.
- A failure in the **AI provider** restricts only operations that route through that provider; routing fallback to local model continues if local model is healthy.
- A failure in **one adapter** restricts only operations using that adapter; other adapters continue.

### §8.3 No silent suppression

Anti-suppression is a constitutional rule:

- A failed component's evidence emission is itself protected by §4.1 row 8: if the evidence log is unavailable, every action is FAIL_CLOSED. There is no "best effort" mode where actions proceed without evidence.
- An adapter that suppresses its own failure (returns success when it actually failed) is detected at L9.2 verification: if verification disagrees with the adapter, the action is FAILED regardless of the adapter's claim. This is per S2.4 §6.4.
- Kernel-side evidence emission for catastrophic failures (TAMPER_DETECTED, BACKEND_VERSION_MISMATCH) bypasses userspace channels; even a fully compromised userspace cannot suppress these records. (Mechanism: the L1 kernel has direct access to a small reserved evidence-log segment for catastrophic events.)

### §8.4 Evidence emission must not itself fail open

If evidence emission for a failure would itself fail (chain of doom), the runtime must:

1. attempt to write to a reserved in-memory ring buffer,
2. transition to RECOVERY_PENDING,
3. halt all mutations,
4. flush the ring buffer to the recovery-mode evidence path on next recovery boot.

There is no path where a failure is unobserved.

### §8.5 TAMPER_DETECTED never propagates upward

A TAMPER_DETECTED in any layer halts that layer's dependent layers (downward) but **never** propagates upward beyond evidence emission. The reason: a tamper signal from a lower-trust layer (e.g. an adapter) cannot cause a higher-trust layer (e.g. policy kernel) to halt itself. Otherwise an attacker controlling an adapter could deny service to the rest of the system. Tamper signals are evidence-only at the upward boundary; the receiving higher-trust layer logs and continues.

## §9 Adversarial robustness

The failure-handling machinery is itself a target. This section enumerates the threat surface and the mitigations.

### §9.1 Threat — cascading failure as denial-of-service

**Attack:** an adversary triggers a failure that cascades through retries, restarts, and circuit breakers, exhausting resources until the system halts.

**Mitigation:** every retry budget is bounded (§6.3). Circuit breakers prevent unbounded outbound retry. Restart budgets prevent unbounded component restarts. Recovery loop detection (§6.4) prevents repeated automated recovery boots. The maximum resource consumption from cascading failures is bounded a priori by the closed budgets in this spec.

### §9.2 Threat — false-positive failure detection

**Attack:** an adversary triggers spurious "failures" that are not actually failures, causing degradation transitions and operator fatigue.

**Mitigation:** verification-engine probe failures distinguish `VERIFICATION_FAILED` (the predicate failed — real failure) from `VERIFICATION_PROBE_ERROR` (the probe broke — not a verification failure) per S2.4. The runtime uses `VERIFICATION_FAILED` for state-change escalation; `VERIFICATION_PROBE_ERROR` does not move DegradationLevel by itself (it raises an alert but not a level transition). This is the same discipline applied throughout: a "failure" of a probe to evaluate is not the same as the probed predicate being false.

### §9.3 Threat — failure suppression by malicious adapter

**Attack:** a compromised L3 adapter suppresses its own failure indications, allowing the runtime to believe the action succeeded when it did not.

**Mitigation:** S2.4 verification is run independently of the adapter and emits its own evidence. The action lifecycle (S0.1) does not transition to `succeeded` based on adapter claims alone — verification must pass. Kernel-side evidence emission (§8.3) catches userspace suppression for catastrophic events. An adapter that lies about success is detected within one verification cycle.

### §9.4 Threat — spurious recovery loops

**Attack:** an adversary triggers conditions that cause repeated automated recovery boots, denying service.

**Mitigation:** §6.4 recovery loop detection. After N=3 entries with the same `RecoveryEntryReason` within M=60 minutes, the system halts and waits for operator intervention. The threshold is bounded.

### §9.5 Threat — evidence log saturation

**Attack:** an adversary triggers a high-volume failure pattern that saturates the evidence log, slowing legitimate operations or causing log rotation that drops important records.

**Mitigation:** rate-limiting on `FAILURE_OBSERVED` per (FailureClass, layer) — beyond N records per minute, the runtime emits one `FAILURE_OBSERVED_RATE_LIMITED` summary record (STANDARD*24M) and suppresses further per-event records for the cool-down. Summary records carry the count and class. This is a \_noisy* suppression — suppression itself is logged. FOREVER-retention records (TAMPER_DETECTED, BUNDLE_REJECTED, BACKEND_VERSION_MISMATCH, COMPONENT_RESTART_BUDGET_EXHAUSTED, RECOVERY_LOOP_DETECTED) are **never** rate-limited; saturation cannot drop them.

### §9.6 Threat — DegradationLevel manipulation by AI

**Attack:** an AI subject attempts to write the DegradationLevel field to mask a degraded condition from operators or to claim degraded conditions to justify a deviation.

**Mitigation:** DegradationLevel is read-only to all subjects with `is_ai = true`. Writes are restricted to L0-rule-driven runtime code via an internal capability gated by S2.3 hard-deny `LevelManipulationByAi`. The level reflects observed component state, not subject claims.

### §9.7 Threat — runbook tampering

**Attack:** an adversary modifies runbook content to mislead the operator during an incident.

**Mitigation:** runbook AIOS-FS objects carry the standard L2 signature chain. Recovery-mode reads use the read-only forensic attach. The runtime emits a `runbook_hash` field in `FAILURE_OBSERVED`; the operator UI displays this and the hash as it appears now, so substitution is detectable. The runbook bundle itself can be a signed L4-managed bundle in future revisions; for now the L2 signature chain is sufficient.

### §9.8 Threat — circuit-breaker stuck open

**Attack:** an adversary keeps a circuit breaker open by ensuring the half-open probe always fails, denying service to a target indefinitely.

**Mitigation:** circuit breakers have a _maximum_ cool-down per §6.2. Beyond max cool-down, the breaker emits a continuous `CIRCUIT_BREAKER_OPENED` record (rate-limited) and the operator is alerted. The breaker does not stay closed if the underlying target is unhealthy, but the alerting ensures the operator is aware. There is no automatic forcing closed by the runtime.

### §9.9 Threat — degradation on policy bundle update

**Attack:** an adversary races two policy bundle updates such that the second one fails verification while in-flight, leaving the system in a degraded state with the older bundle.

**Mitigation:** the policy kernel atomically rejects failed-verification bundles — there is no partial application. The kernel either fully adopts the new bundle or fully retains the previous one. `POLICY_BUNDLE_REJECTED` is emitted; degradation level reflects only that the _update was rejected_, not that the policy state is internally inconsistent. The system continues to evaluate policy from the still-valid previous bundle.

## §10 Telemetry contract

Bounded-cardinality metrics per L9 conventions.

### §10.1 Closed metrics

| Metric                                     | Type    | Labels                                                       |
| ------------------------------------------ | ------- | ------------------------------------------------------------ |
| `failure_observed_total`                   | counter | `failure_class`, `layer`                                     |
| `degradation_level_active`                 | gauge   | `level`                                                      |
| `degradation_level_transition_total`       | counter | `from_level`, `to_level`                                     |
| `recovery_entry_total`                     | counter | `entry_reason` (closed enum from S9.1)                       |
| `circuit_breaker_open_total`               | counter | `component`                                                  |
| `circuit_breaker_state_active`             | gauge   | `component`, `state`                                         |
| `tamper_detected_total`                    | counter | `tamper_class`, `layer`                                      |
| `component_restart_total`                  | counter | `component`                                                  |
| `component_restart_budget_exhausted_total` | counter | `component`                                                  |
| `bundle_rejected_total`                    | counter | `bundle_kind` (invariant/policy/identity/capability/sandbox) |
| `recovery_loop_detected_total`             | counter | `entry_reason`                                               |

### §10.2 Cardinality bounds

- `failure_class` ≤ 15 (closed enum)
- `layer` ≤ 11 (closed enum L0..L10)
- `level` ≤ 6 (closed `DegradationLevel`)
- `from_level`, `to_level` ≤ 6 each
- `entry_reason` per S9.1 closed enum (≤ 12)
- `component` ≤ 12 (canonical component list per §6.1; new components require this list update)
- `state` (circuit breaker) ∈ {closed, open, half_open} = 3
- `tamper_class` ≤ 6 (per S3.1 sub-classification)
- `bundle_kind` ∈ {invariant, policy, identity, capability, sandbox} = 5

### §10.3 Forbidden labels

The following are **never** allowed as metric labels:

- `subject_id`
- `group_id`
- `user_id`
- `action_id`
- `receipt_id`
- `verification_id`
- `path` (free-form filesystem path)
- `endpoint` (free-form URL)
- any free-form string

These would be unbounded cardinality and break the L9 budget. Subject and action IDs live in evidence records; aggregation lives in the L9 query layer, not in metrics.

### §10.4 Metric lifetime

Metrics are emitted continuously while the host is running. On reboot into recovery, metrics are reset; recovery-mode metrics are emitted under a separate prefix `recovery_*` per S9.1 telemetry conventions.

## §11 Worked examples

Three concrete examples illustrating the table mapping, the FSM transitions, and the evidence trail.

### §11.1 Example A — tampered policy bundle, recover to known-good

**Scenario:** an operator submits a new policy bundle. The bundle's Ed25519 signature has been tampered with in transit (header bit flipped). The policy kernel attempts to load the bundle.

**Sequence:**

1. Policy kernel receives `LoadPolicyBundle(bundle_v42)` request.
2. Signature verification fails (Ed25519 mismatch).
3. Policy kernel applies §4.1 row 2: `BUNDLE_SIGNATURE_FAILURE × L4.1 → DEGRADE_TO_KNOWN_GOOD`.
4. Policy kernel emits `POLICY_BUNDLE_REJECTED` (FOREVER) with `bundle_version = v42`, `signing_key_id`, `failure_reason = SignatureMismatch`.
5. Policy kernel continues using `bundle_v41` (the previously-loaded known-good bundle).
6. DegradationLevel: a soft degradation is observed because a bundle update was rejected, but operations continue normally on `v41`. Level transitions `NORMAL → DEGRADED_SOFT`. `DEGRADATION_LEVEL_TRANSITIONED` (STANDARD_24M) emitted.
7. Operator alerted via the trust bar (level visible) and via the L9 admin surface (alert on `bundle_rejected_total`).
8. Operator investigates, retrieves a correctly-signed `bundle_v43`, submits.
9. Policy kernel verifies signature, loads `v43`. Emits `POLICY_BUNDLE_LOAD` (per S3.1 §4).
10. Health probes confirm no other degradations active. Level returns `DEGRADED_SOFT → NORMAL`. `DEGRADATION_LEVEL_TRANSITIONED` emitted.

**No recovery boot required.** The system never lost the ability to evaluate policy; only the update was rejected. This illustrates why DEGRADE_TO_KNOWN_GOOD is the correct behavior for non-catastrophic bundle failures.

### §11.2 Example B — local AI model crashes mid-session, fall back to direct path

**Scenario:** the L5 local model runner (Ollama-compatible) crashes during a user session. The user has just submitted a request "open the firewall ports for SSH".

**Sequence:**

1. L5 cognitive core attempts to translate the utterance via the local model.
2. Local model RPC returns connection refused.
3. L5 detects `AI_PROVIDER_UNAVAILABLE` × local-model context → §4.1 row 15: DEGRADE_TO_KNOWN_GOOD (no-LLM direct path).
4. L5 emits `FAILURE_OBSERVED` (STANDARD_24M) with `failure_class = AI_PROVIDER_UNAVAILABLE`, `layer = L5`, `provider_id = local`.
5. L5 routes the utterance to the no-LLM direct translation path (per S1.1) — a deterministic capability-name match. The utterance "open the firewall ports for SSH" is matched to `firewall.open_port` with conservative defaults; risk hint is raised because the direct path is less confident than LLM routing.
6. The translated action goes through the standard policy decision pipeline. Because the risk hint is raised, the policy kernel routes it to `REQUIRE_APPROVAL`.
7. DegradationLevel transitions `NORMAL → DEGRADED_SOFT`. `DEGRADATION_LEVEL_TRANSITIONED` emitted. Renderer trust bar shows DEGRADED_SOFT with reason "AI provider unavailable".
8. The operator approves the action (or denies). Action proceeds normally from there.
9. In parallel, the runtime restart-budget for the local model runner allows up to 5 restarts in 10 minutes (§6.1). The runtime attempts a restart at t+5s. Restart succeeds.
10. L5 health probe confirms local model healthy. DegradationLevel returns `DEGRADED_SOFT → NORMAL` after the new probe interval.

**No recovery boot required.** The user's request is served without the AI translator, demonstrating INV-001's principle in microcosm: AI is optional, not required.

### §11.3 Example C — evidence chain hash mismatch, defer to recovery

**Scenario:** a routine scheduled audit runs the S2.4 property check `EVIDENCE_HASH_CHAIN_INTACT`. The audit finds that segment `seg_2026_05_01_0345Z` has a `previous_receipt_hash` mismatch at sequence 1428: the stored value does not match the recomputed BLAKE3 of the prior record.

**Sequence:**

1. S2.4 property check returns `VERIFICATION_FAILED` with `reason_code = HashChainBroken`, `observed.broken_at_segment = "seg_2026_05_01_0345Z"`.
2. The runtime detects this is a TAMPER_DETECTED situation per §4.1 row 27.
3. Runtime emits `TAMPER_DETECTED` (FOREVER) record with full payload: segment id, anomalous sequence, expected hash, observed hash, detection method `scheduled_audit`.
4. Runtime applies BehaviorOnFailure = DEFER_TO_RECOVERY.
5. DegradationLevel transitions `NORMAL → RECOVERY_PENDING`. `DEGRADATION_LEVEL_TRANSITIONED` emitted.
6. New action submissions are rejected with `FAILURE_OBSERVED` and FailureClass = TAMPER_DETECTED in the rejection envelope. In-flight actions either complete or fail naturally; no new ones are accepted.
7. Operator sees the trust bar in RECOVERY_PENDING state. Operator decides to reboot.
8. Reboot into S9.1 recovery boot path. `RecoveryEntryReason = TAMPER_DETECTED`. `RECOVERY_BOOT_ENTERED` (FOREVER) emitted at recovery boot.
9. In recovery, the operator runs forensic analysis: examines the chain, identifies the tampered segment, decides the fix path (restore from backup, mark segment as compromised but quarantined, etc.).
10. Operator exits recovery (per S9.1 exit rules). On normal-mode boot, the system starts in NORMAL provided the audit now passes; otherwise it returns to RECOVERY_PENDING.

**Recovery boot required.** Tamper of evidence is a constitutional violation; the only safe response is to halt mutations and let the operator decide. Notice that the TAMPER_DETECTED record is emitted even though the system is going down — kernel-side evidence emission (§8.3) ensures it survives.

## §12 Evidence record types queued for S3.1 consolidation

This spec queues the following record types for inclusion in the S3.1 closed `RecordType` enum at the next consolidation. None is added unilaterally to S3.1; the actual enum bump and IDL reconciliation is S3.1's responsibility.

| Record type                          | Retention class | Purpose                                                                         |
| ------------------------------------ | --------------- | ------------------------------------------------------------------------------- |
| `FAILURE_OBSERVED`                   | STANDARD_24M    | Generic failure observation. Carries `FailureClass`, layer, runbook reference.  |
| `DEGRADATION_LEVEL_TRANSITIONED`     | STANDARD_24M    | Records the transition (`from`, `to`, triggering FailureClass).                 |
| `COMPONENT_RESTARTED`                | STANDARD_24M    | Per-restart record for a managed component.                                     |
| `COMPONENT_RESTART_BUDGET_EXHAUSTED` | FOREVER         | Restart budget exhausted; defines the moment of recovery escalation.            |
| `CIRCUIT_BREAKER_OPENED`             | EXTENDED_60M    | Breaker opened; carries target, failure count, cool-down.                       |
| `CIRCUIT_BREAKER_CLOSED`             | STANDARD_24M    | Breaker closed; carries target, time-open.                                      |
| `HALTED_PENDING_OPERATOR`            | FOREVER         | System entered HALTED; carries triggering FailureClass and chain of escalation. |
| `TIME_DRIFT_DETECTED`                | EXTENDED_60M    | Wall-clock drift exceeded tolerance; carries observed skew and tolerance.       |
| `BACKEND_VERSION_MISMATCH`           | FOREVER         | Boot-time substrate version mismatch.                                           |
| `RECOVERY_LOOP_DETECTED`             | FOREVER         | N entries in M minutes for the same RecoveryEntryReason.                        |

The record-payload IDLs follow the S3.1 §4 pattern. Sketches:

```proto
message FailureObservedPayload {
  aios.failure.v1alpha1.FailureClass failure_class = 1;
  string layer = 2;                                // "L4.1", "L9.1", ...
  string component_id = 3;
  string runbook_path = 4;                         // "/aios/system/runbooks/.../index.md"
  string runbook_hash = 5;
  bool   runbook_present = 6;
  string failure_detail_redacted = 7;              // implementation-specific, redacted per S3.1
  string coverage = 8;                             // "table_row" | "uncovered"
  uint32 table_row = 9;                            // §4.1 row number, 0 if uncovered
}

message DegradationLevelTransitionedPayload {
  aios.failure.v1alpha1.DegradationLevel from_level = 1;
  aios.failure.v1alpha1.DegradationLevel to_level   = 2;
  aios.failure.v1alpha1.FailureClass triggering_class = 3;
  string triggering_component_id = 4;
}

message ComponentRestartedPayload {
  string component_id = 1;
  uint32 restart_index_in_window = 2;
  uint32 budget_window_seconds = 3;
  uint32 budget_window_max = 4;
}

message CircuitBreakerOpenedPayload {
  string target_id = 1;
  uint32 failure_count = 2;
  google.protobuf.Duration cooldown = 3;
}

message HaltedPendingOperatorPayload {
  aios.failure.v1alpha1.FailureClass triggering_class = 1;
  repeated string escalation_chain = 2;             // ordered list of FailureClass values
  string final_evidence_receipt_id = 3;
}

message TimeDriftDetectedPayload {
  google.protobuf.Duration observed_skew = 1;
  google.protobuf.Duration tolerance = 2;
  string source_clock = 3;
}

message BackendVersionMismatchPayload {
  string substrate_kind = 1;                        // "kernel" | "firmware" | "aios_substrate"
  string observed_version = 2;
  string required_version = 3;
}

message RecoveryLoopDetectedPayload {
  string entry_reason = 1;                          // S9.1 RecoveryEntryReason value
  uint32 entries_in_window = 2;
  uint32 window_minutes = 3;
}
```

The `BUNDLE_REJECTED` records (`INVARIANT_BUNDLE_REJECTED`, `POLICY_BUNDLE_REJECTED`, `IDENTITY_BUNDLE_REJECTED`, `CAPABILITY_BUNDLE_REJECTED`, `SANDBOX_BUNDLE_REJECTED`) are queued in their respective producing specs (L0, S2.3, L4.3, S1.1, S6.3) — this spec only references them; it does not redeclare them.

## §13 Acceptance criteria

A failure-handling implementation satisfies this spec when **all** of the following hold:

1. Every `FailureClass` value in §3.1 maps to at least one row in §4.1 OR is documented as explicitly handled by the default behavior (FAIL_CLOSED + DEGRADED_SOFT).
2. The DegradationLevel FSM (§5.1) is implemented exactly as specified — no other transitions exist, no transition is missing.
3. Circuit-breaker budgets (§6.1, §6.2) match the listed values; tests demonstrate budget exhaustion behaves as specified.
4. The recovery-loop detector (§6.4) trips at the listed thresholds.
5. Runbook lookup (§7) resolves through the canonical AIOS-FS path; missing runbooks are reported, not papered over.
6. Anti-cascade rules (§8) are observable via tests: a failed L3 adapter does not cause the L4 policy kernel to restart; a failed evidence log halts mutations; etc.
7. Adversarial scenarios (§9) are covered by integration tests including: rate-limited failure floods do not drop FOREVER records; verification probe errors are distinguished from verification failures; an AI subject cannot write the level field.
8. Telemetry metrics (§10) are emitted with bounded label cardinality; no forbidden labels appear; cardinality budget is enforced at metric registration time.
9. The three worked examples (§11) reproduce step-by-step in an integration test.
10. All ten queued evidence records (§12) are emitted at the correct retention class on the corresponding triggers.
11. No code path emits an action's `succeeded` envelope without verification passing — INV-014 enforced through this spec's failure machinery.

## §14 Cross-spec dependencies

| Spec                                        | Direction | Relationship                                                                                                              |
| ------------------------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------------- |
| **L0 INV-014** (no proof no completion)     | consumer  | This spec's failure-handling makes INV-014 mechanically enforceable: actions without successful verification fail closed. |
| **L0 INV-001** (recovery independent of L5) | consumer  | The DEGRADE_TO_KNOWN_GOOD path for AI provider unavailability honors INV-001 by serving the user without L5.              |
| **L0 INV-007** (acyclic layer dependency)   | consumer  | §8.1 propagation rule.                                                                                                    |
| **L0 INV-008** (default deny)               | consumer  | FAIL_CLOSED is the default behavior in §3.3 — the failure-domain projection of default deny.                              |
| **S9.1** (recovery boundary)                | consumer  | DEFER_TO_RECOVERY behavior reads `RecoveryEntryReason`; recovery-loop detector cites S9.1 enums.                          |
| **S2.3** (policy kernel)                    | consumer  | Bundle signature failure handling (row 2); decision timeout handling (row 17); hard-deny `LevelManipulationByAi`.         |
| **S2.4** (verification grammar)             | consumer  | Verification timeout (row 18); probe-error vs verification-fail distinction (§9.2).                                       |
| **S3.1** (evidence log)                     | consumer  | Ten new record types queued (§12); evidence log unavailability handling (row 8).                                          |
| **S5.2** (vault broker)                     | consumer  | Vault unavailability (row 14); TPM-unavailable post-boot (row 22).                                                        |
| **S13.1** (cognitive core)                  | consumer  | AI provider unavailability (rows 15, 16); agent DEGRADED state surfaces level.                                            |
| **S10.1** (capability runtime)              | consumer  | Adapter failure (row 19); queue depth backpressure (row 26); capability runtime unavailability (row 10).                  |
| **S8.1** (network policy)                   | consumer  | Network partition handling (row 20).                                                                                      |
| **S8.2** (GPU resource model)               | consumer  | GPU disconnect handling (row 21).                                                                                         |
| **L4.3** (identity model)                   | consumer  | Identity service unavailable (row 7); identity bundle rejected (row 3).                                                   |
| **L1** (kernel bootstrap)                   | consumer  | Backend version mismatch at boot (row 30); recovery operator unavailable (row 31).                                        |
| **S4.1** (namespace layout)                 | consumer  | Runbook lookup path resolves through the active namespace catalog.                                                        |
| **L7.2** (shared UI schema)                 | consumer  | DegradationLevel renders in trust bar per L0 INV-020.                                                                     |
| **L9 telemetry pipeline** (future S14.2)    | producer  | Metrics declared in §10 are projected by L9 telemetry; cardinality budget enforced at registration time.                  |

## §15 Open deferrals

The following are intentionally not handled in this revision and are queued for future sub-specs:

- **Multi-host failure correlation.** Single-host now. A multi-host AIOS deployment will need a cross-host failure-class taxonomy and propagation rules. Single-host behavior is fully closed in this rev.
- **User-customizable runbook severity tags.** Runbooks carry a fixed `severity` field; per-tenant or per-operator severity preferences are out of scope.
- **Automated remediation actions.** This spec does not include automated remediation (e.g. "if disk full, run garbage collection"). Automated remediation is itself a state-changing action and would go through the standard policy + verification pipeline; sketching it is deferred.
- **Historical degradation analytics.** Aggregated incident analytics (mean time between failures, mean time to recovery, per-component reliability) are L9 query-layer concerns, not failure-handling concerns.
- **Failure injection for testing.** A signed failure-injection capability for integration testing is sketched but not contracted here.
- **Cross-bundle rejection correlation.** When two bundles (e.g. policy + identity) are signed by the same compromised key, the system currently rejects each independently. A cross-bundle correlation that infers key compromise from multiple rejections is queued.

## §16 Per-FailureClass detection mechanics

This section is a closed table specifying _how_ each FailureClass is detected. Detection is deterministic: there is exactly one canonical detector per (FailureClass, layer-context) pair. A second-source detector is allowed (defense in depth) but the canonical detector is the source of truth for the evidence record.

### §16.1 Detection table

| FailureClass                  | Canonical detector                                                                        | Detection latency target | Second-source detector                    |
| ----------------------------- | ----------------------------------------------------------------------------------------- | ------------------------ | ----------------------------------------- |
| COMPONENT_UNAVAILABLE         | Per-component health probe (gRPC ping or unix-domain socket reachability) at 5 s interval | < 10 s                   | Caller-side connection error              |
| COMPONENT_DEGRADED            | Component self-report via health surface (`degraded_reason` field)                        | < 1 s                    | External capability probe                 |
| BUNDLE_SIGNATURE_FAILURE      | The bundle-loading code in the receiving service (policy kernel, identity service, etc.)  | < 1 s                    | Operator-initiated re-verify              |
| HARDWARE_FAILURE              | Kernel events (udev, device-removed) + S2.4 property check                                | < 5 s                    | Driver health probe                       |
| VAULT_UNAVAILABLE             | L4.2 vault broker self-report on RPC                                                      | < 5 s                    | Caller-side broker connection error       |
| AI_PROVIDER_UNAVAILABLE       | L5 cognitive core call returns connection-refused or 5xx                                  | < 1 s                    | Periodic health probe                     |
| POLICY_DECISION_TIMEOUT       | Policy kernel timer expiry on EvaluatePolicy                                              | exactly the budget       | Caller-side deadline                      |
| VERIFICATION_TIMEOUT          | Verification engine per-primitive timer (per S2.4 §6.3)                                   | exactly the budget       | Caller-side deadline                      |
| ADAPTER_FAILURE               | Capability runtime detects panic (process exit) or unparseable error envelope             | < 1 s                    | Verification disagreement                 |
| NETWORK_PARTITION             | L8 network plane health probe (gateway reachability)                                      | < 30 s                   | Outbound connection error rate threshold  |
| TAMPER_DETECTED               | S2.4 property check `EVIDENCE_HASH_CHAIN_INTACT` scheduled run                            | per audit cadence        | Manual operator re-verify; bundle re-load |
| RESOURCE_EXHAUSTION           | OS-level metrics (df, free memory, queue depth) read by L9 telemetry                      | < 30 s                   | Component-side allocation failure         |
| TIME_DRIFT                    | NTP daemon comparison + L0 wall-clock comparison at signature TTL evaluation              | < 60 s                   | Multi-source clock comparison             |
| BACKEND_VERSION_MISMATCH      | L1 boot-path version check before /aios mount                                             | exactly at boot          | First L4 bundle load                      |
| RECOVERY_OPERATOR_UNAVAILABLE | L1 recovery boot-path operator-credential check at STAGE_RECOVERY_SHELL_READY             | exactly at recovery      | Repeated attempt timeout                  |

### §16.2 Detection-latency contract

The "Detection latency target" column is a soft target; it is not a per-spec invariant for all installations. It is the design target for default health-probe intervals. Operators may tune intervals per-component (within bounds set by L9 telemetry) but cannot disable detection. The longer the detection latency, the longer the window during which a failure goes unrecorded — there is no such thing as "no detection".

### §16.3 No detector silence

A detector that produces no evidence for an extended period is itself a failure surface — an attacker could disable a detector and then exploit the unmonitored window. Mitigation:

- **Heartbeat records.** Each canonical detector emits a `DETECTOR_HEARTBEAT` (in-memory, telemetry-only — not evidence) at its probe interval. L9 telemetry monitors heartbeat continuity.
- **Missing heartbeats are themselves a FailureClass = COMPONENT_UNAVAILABLE event** for the detector itself, recorded normally per §4.1 row 6 or row 9.
- **Heartbeat suppression** by a compromised detector is detected by the second-source mechanism listed in §16.1: every FailureClass has a non-canonical second detector that catches false-negative canonical detection.

### §16.4 Detector authority discipline

A detector may emit only the FailureClass it is authoritative for. The capability runtime cannot emit `BUNDLE_SIGNATURE_FAILURE`; only the bundle-loading code in the policy kernel (or other bundle-receiving service) is authoritative. This discipline mirrors the S3.1 "callers may emit only payload types matching their authority" rule.

The L4 policy authoring layer can express this as a hard-deny rule of the form: "Subject `verification_engine` cannot emit a `BUNDLE_SIGNATURE_FAILURE` evidence record." The default L0 invariant bundle includes the canonical detector authority list as part of its bundled config.

## §17 Failure observability narratives

This section describes the operator-facing narratives the L9 admin surface and the renderer trust bar use to explain failures. The narratives are templated, not free-form: each (FailureClass, DegradationLevel) pair maps to a fixed template with bounded substitution slots.

### §17.1 Template structure

Each narrative has three slots:

```text
{{ situation_one_liner }}
{{ what_the_system_did }}
{{ what_the_operator_should_do }}
```

The L9 admin surface fills the slots from the evidence record (FailureClass, DegradationLevel, runbook reference). The renderer trust bar uses an abbreviated form (situation_one_liner only).

### §17.2 Closed template catalog

The template catalog is a versioned bundle (`narrativebundle_<hex>`) signed by the AIOS root. There is one template per (FailureClass, DegradationLevel) pair the system can be in. Adding a template is a versioned bundle update.

| FailureClass × DegradationLevel          | Situation one-liner                       | What the system did                                    | What the operator should do                                       |
| ---------------------------------------- | ----------------------------------------- | ------------------------------------------------------ | ----------------------------------------------------------------- |
| BUNDLE_SIGNATURE_FAILURE × DEGRADED_SOFT | "Policy bundle update rejected"           | "Continued running on the previously approved bundle." | "Verify the new bundle and resubmit when correctly signed."       |
| BUNDLE_SIGNATURE_FAILURE × DEGRADED_HARD | "Constitution bundle update rejected"     | "Reduced to the two minimum constitutional rules."     | "Repair via recovery boot and reload the constitution bundle."    |
| VAULT_UNAVAILABLE × DEGRADED_HARD        | "Secrets are unavailable"                 | "Refused all secret-bearing operations."               | "Investigate vault status; repair TPM if needed."                 |
| AI_PROVIDER_UNAVAILABLE × DEGRADED_SOFT  | "AI assistant is offline"                 | "Switched to direct (no-AI) translation."              | "Restart the local model or check network for external provider." |
| TAMPER_DETECTED × RECOVERY_PENDING       | "Tamper detected on evidence"             | "Halted mutations; awaiting recovery."                 | "Reboot into recovery; perform forensic analysis."                |
| BACKEND_VERSION_MISMATCH × HALTED        | "Substrate version is incompatible"       | "Refused to mount /aios."                              | "Upgrade kernel/firmware or recover via S9.1 path."               |
| RECOVERY_OPERATOR_UNAVAILABLE × HALTED   | "Operator credentials cannot be verified" | "Paused at the recovery shell."                        | "Provide the operator credential or initiate factory reset."      |
| NETWORK_PARTITION × DEGRADED_SOFT        | "Network is partitioned"                  | "Switched to offline mode; LAN exposures revoked."     | "Wait for network return; or investigate the network plane."      |
| RESOURCE_EXHAUSTION × READ_ONLY          | "Disk full on /aios"                      | "Refused new writes; reads continue."                  | "Run garbage collection or expand storage."                       |
| HARDWARE_FAILURE × DEGRADED_HARD         | "Critical hardware failure"               | "Refused operations dependent on the failed hardware." | "Repair or replace the affected hardware."                        |

### §17.3 Narrative discipline

- Narratives are **descriptive**, not prescriptive — they describe what the system did and what the operator should consider. The runbook (per §7) carries the full prescriptive procedure.
- Narratives are **factual** — they do not editorialize, speculate, or apologize. "Reduced to the two minimum constitutional rules" is acceptable; "Sorry, the constitution failed to load" is not.
- Narratives are **bounded-cardinality strings** — they live in the signed narrative bundle and cannot be authored at runtime by an L5 subject. An AI assistant cannot synthesize a custom narrative for the operator; only the bundled templates are rendered.
- Narratives are **localized** — the bundle includes per-locale tables. The rendered narrative respects the active locale, but the substitution slots remain identical.

### §17.4 Narratives are not a hide-the-failure path

The narrative is a _summary_ layered above the evidence record — it does not replace the evidence record. The evidence record carries the full structural detail; the narrative is the renderer-friendly projection. An operator who wants the structural detail uses the L9 admin surface to view the evidence record directly.

## §18 Recovery handoff contract

This section specifies how the failure-handling subsystem hands off to S9.1 recovery boot.

### §18.1 The handoff sequence

When the failure-handling subsystem decides DEFER_TO_RECOVERY (or escalates to RECOVERY_PENDING / HALTED):

1. **Block new mutations.** No new state-changing actions are accepted.
2. **Drain in-flight actions.** Existing actions that have passed `policy_pending → executing` are allowed to complete (succeed, fail, or roll back) but no new transitions to `executing` happen. There is a soft cap on drain time (default 5 minutes); after the cap, in-flight actions are forcibly transitioned to `failed` with `reason_code = ShutdownDrainExceeded`.
3. **Flush evidence.** All in-memory evidence records (including the kernel-side ring buffer per §8.4) are flushed to the evidence log.
4. **Emit handoff record.** A `RECOVERY_HANDOFF_INITIATED` record is emitted with the triggering FailureClass and the full chain of escalations.
5. **Set recovery entry reason.** The S9.1 `RecoveryEntryReason` is set to the FailureClass-derived reason (mapping in §18.2).
6. **Reboot.** The system reboots into the recovery boot path.

### §18.2 FailureClass → RecoveryEntryReason mapping

| FailureClass                                                   | RecoveryEntryReason (S9.1 closed enum)                        |
| -------------------------------------------------------------- | ------------------------------------------------------------- |
| TAMPER_DETECTED                                                | TAMPER_DETECTED                                               |
| BUNDLE_SIGNATURE_FAILURE (L0)                                  | INVARIANT_BUNDLE_REPAIR                                       |
| BACKEND_VERSION_MISMATCH                                       | SUBSTRATE_INCOMPATIBLE                                        |
| HARDWARE_FAILURE (disk on /aios)                               | AIOSFS_REPAIR                                                 |
| HARDWARE_FAILURE (TPM)                                         | VAULT_HARDWARE_REPAIR                                         |
| RECOVERY_OPERATOR_UNAVAILABLE                                  | OPERATOR_CREDENTIAL_REPAIR                                    |
| RESOURCE_EXHAUSTION (disk on /aios after READ_ONLY exhaustion) | STORAGE_REPAIR                                                |
| COMPONENT_RESTART_BUDGET_EXHAUSTED (per §6.1)                  | COMPONENT_REPAIR                                              |
| RECOVERY_LOOP_DETECTED                                         | LOOP_BREAKER_HALT (terminal; no exit until operator override) |

The mapping is closed. A FailureClass that does not appear here cannot trigger a recovery boot directly; it must escalate through one of the listed paths or to HALTED.

### §18.3 Handoff is irreversible

A DEFER_TO_RECOVERY decision, once committed, cannot be reversed by the failure-handling subsystem. There is no "actually recovered, never mind" path. The reason: the failure that caused DEFER_TO_RECOVERY may be intermittent; if the handoff were reversible, an attacker could trigger the failure, reverse the handoff, and exploit the brief window. Reversal is only via reboot (which exits and re-enters the FSM at NORMAL).

### §18.4 Handoff evidence chain

The recovery boot sees the evidence chain as it was at handoff. The first record after recovery boot is `RECOVERY_BOOT_ENTERED` (per S9.1) with `previous_normal_mode_terminal_receipt_id` referencing the last normal-mode evidence record. This is how forensic analysis ties the recovery session to the failure that triggered it.

## §19 Anti-patterns explicitly rejected

This section names the anti-patterns observed in other systems and explicitly rejects them for AIOS.

### §19.1 "Soft accept" is not a behavior

There is no `BehaviorOnFailure = SOFT_ACCEPT` value. A bundle whose signature failed is **not** loaded with a warning — it is rejected. A vault that is unavailable does **not** allow the action to proceed with a warning — the action is FAIL_CLOSED. The "warning, but continue" pattern is the most common path to constitutional erosion in audited systems and is forbidden here.

### §19.2 "Best effort" is not a behavior

There is no path where evidence emission is "best effort" and an action proceeds without it. Every action emits evidence or does not run. INV-014 enforced.

### §19.3 "Self-healing without record" is not a behavior

A component that recovers from a transient failure must still emit `FAILURE_OBSERVED` for the failure event. Silent self-recovery is forbidden because it hides the failure from the operator and from forensic analysis.

### §19.4 "Operator override of FAIL_CLOSED without recovery" is not allowed

An operator cannot, in normal mode, force a FAIL_CLOSED action to proceed. Force is a recovery-mode operation. This is the same discipline that S5.4 emergency override applies to high-risk actions: bypassing default-deny requires the operator to be in recovery mode.

### §19.5 "Auto-promote to NORMAL after silence" is not allowed

DegradationLevel does not auto-relax to NORMAL based on the absence of recent failures. Auto-relax requires the _underlying cause_ to be observed cleared. A 30-second silence on a degraded component does not mean the component is healthy; only a successful health probe does.

### §19.6 "Free-form failure reason" is not allowed

Every failure has a closed FailureClass. There is no `failure_class = "other"` field. There is no `failure_reason = "<arbitrary string>"` payload. Failure detail goes into a typed payload field; the class is one of the closed enum values.

### §19.7 "Fail-fast without evidence" is not allowed

A FAIL_CLOSED behavior must still emit the evidence record. "Fail fast and quietly" is a recipe for unobservable failures. A failed action emits evidence; an action that produces no evidence did not happen — and therefore did not fail either.

## §20 gRPC service surface

The failure-handling subsystem exposes a small read-mostly gRPC service for the L9 admin surface and renderers. There is no write API for FailureClass, DegradationLevel, or behavior — those are emitted by canonical detectors per §16, never set by an external caller.

```proto
syntax = "proto3";
package aios.failure.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/duration.proto";
import "google/protobuf/empty.proto";

service FailureSurface {
  rpc GetCurrentLevel(google.protobuf.Empty) returns (CurrentLevelResponse);
  rpc ListActiveFailures(ListActiveFailuresRequest) returns (ListActiveFailuresResponse);
  rpc GetCircuitBreakerState(GetCircuitBreakerStateRequest) returns (CircuitBreakerStateResponse);
  rpc GetRestartBudgetState(GetRestartBudgetStateRequest) returns (RestartBudgetStateResponse);
  rpc GetRunbookForClass(GetRunbookForClassRequest) returns (RunbookHeaderResponse);
  rpc StreamLevelTransitions(google.protobuf.Empty) returns (stream DegradationLevelTransitionedPayload);
  rpc StreamFailureObservations(StreamFailureObservationsRequest) returns (stream FailureObservedPayload);
}

message CurrentLevelResponse {
  DegradationLevel level = 1;
  google.protobuf.Timestamp since = 2;
  repeated string contributing_failure_ids = 3;
}

message ListActiveFailuresRequest {
  FailureClass filter_by_class = 1;        // UNSPECIFIED = all
  string filter_by_layer = 2;              // empty = all
}

message ActiveFailure {
  string failure_id = 1;
  FailureClass failure_class = 2;
  string layer = 3;
  string component_id = 4;
  google.protobuf.Timestamp first_observed_at = 5;
  google.protobuf.Timestamp last_observed_at = 6;
  uint32 observation_count = 7;
  string runbook_path = 8;
}

message ListActiveFailuresResponse {
  repeated ActiveFailure failures = 1;
}

message GetCircuitBreakerStateRequest { string component = 1; }

enum CircuitBreakerState {
  CIRCUIT_BREAKER_STATE_UNSPECIFIED = 0;
  CIRCUIT_BREAKER_CLOSED            = 1;
  CIRCUIT_BREAKER_HALF_OPEN         = 2;
  CIRCUIT_BREAKER_OPEN              = 3;
}

message CircuitBreakerStateResponse {
  string component = 1;
  CircuitBreakerState state = 2;
  google.protobuf.Timestamp opened_at = 3;
  google.protobuf.Duration cooldown_remaining = 4;
  uint32 consecutive_failures = 5;
}

message GetRestartBudgetStateRequest { string component = 1; }

message RestartBudgetStateResponse {
  string component = 1;
  uint32 restarts_in_window = 2;
  uint32 budget_max = 3;
  google.protobuf.Duration window = 4;
  bool exhausted = 5;
}

message GetRunbookForClassRequest { FailureClass failure_class = 1; }

message RunbookHeaderResponse {
  string schema_version = 1;
  FailureClass failure_class = 2;
  repeated string applies_to_layers = 3;
  string severity = 4;
  string expected_recovery_path = 5;
  string estimated_resolution_time = 6;
  string last_reviewed_at = 7;
  string runbook_path = 8;
  bool runbook_present = 9;
  string runbook_hash = 10;
}

message StreamFailureObservationsRequest {
  FailureClass filter_by_class = 1;
  string filter_by_layer = 2;
}
```

### §20.1 Read-only by construction

The service has no `Set*` or `Force*` RPCs. There is no `OverrideDegradationLevel`, no `ForceCloseCircuitBreaker`, no `ResetRestartBudget`. Mutation of failure-handling state is exclusively driven by canonical detectors emitting evidence records, which the runtime aggregates into the surface. This is intentional: a write API would be a constitutional bypass channel.

### §20.2 Streaming surfaces

`StreamLevelTransitions` and `StreamFailureObservations` are the live channels the admin surface and the renderer trust bar consume. They are server-streaming RPCs; subscribers cannot back-pressure. If a subscriber falls behind, it is dropped and reconnects from the latest evidence sequence number — there is no replay-from-history on these streams (replay is via the S3.1 evidence query API).

### §20.3 Authority

All `FailureSurface` RPCs require subjects with `is_ai = false` and the `failure_surface.read` capability. AI subjects cannot read the failure surface directly — they can only observe DegradationLevel through their session enrichment. This is so that AI subjects cannot use the failure surface as a side-channel to detect operator intent or trigger reactive degradation games.

## §21 Edge-case considerations

This section catalogs edge cases that are easy to overlook and specifies the spec's stance on each.

### §21.1 Failure during failure handling

If the failure-handling subsystem itself fails (e.g. the evidence emitter for `FAILURE_OBSERVED` panics), the runtime falls to the kernel-side reserved channel (§8.4) and transitions to RECOVERY_PENDING. There is no infinite recursion: a failure of the failure-handling code is an unobserved-failure surface and the only safe response is to halt mutations.

### §21.2 Two failures, one component

If a component observes two distinct FailureClasses simultaneously (e.g. policy kernel observes both `BUNDLE_SIGNATURE_FAILURE` for an incoming bundle update _and_ `RESOURCE_EXHAUSTION` for OOM), each is emitted as its own record and each contributes to the DegradationLevel separately. The level is the maximum (most-restrictive) implied by any active failure. There is no "combined class".

### §21.3 Same failure on multiple hosts (multi-host deferral)

Multi-host failure correlation is deferred (§15). On a single host, the same failure observed twice within a short window emits two records; rate-limiting (§9.5) suppresses the third+ within the cool-down with a summary record. The level is set by the first record; subsequent records do not re-transition the level.

### §21.4 Failure observed during a recovery boot

A failure observed during a recovery boot (e.g. the recovery-mode evidence write fails) is emitted into the recovery-mode evidence channel (per S9.1) and recorded with the special `recovery_mode = true` flag. Recovery-mode failures escalate to HALTED if they prevent the recovery boot from completing; otherwise they accumulate as recovery-session evidence.

### §21.5 Failure during a simulated/dry-run action

S0.1 dry-run actions can also fail (e.g. translator timeout during a dry-run). Failures during simulated actions emit evidence with `simulated = true` (per S3.1 §3) and contribute to telemetry but do **not** transition DegradationLevel. The reason: simulated actions are caller-driven what-if experiments; their failures are inherent to the simulation request, not the system's actual operating state.

### §21.6 Slow-running components mistaken for unavailable

A canonical detector that uses a 5 s probe interval may transiently see a slow component as unavailable. Mitigation: detector consecutive-failure threshold (default 3 consecutive failures before classification as `COMPONENT_UNAVAILABLE`). A single missed probe is logged but does not transition state. Tunable per-component, lower-bounded at 2 (cannot be disabled to "1 missed probe = unavailable").

### §21.7 Bundle accepted then later observed tampered

A bundle accepted at load (signature verified at the time) but observed tampered later (post-load corruption) is `TAMPER_DETECTED`, not `BUNDLE_SIGNATURE_FAILURE`. The distinction matters: the former indicates the load process worked correctly and something corrupted memory afterward (suggesting a memory or storage compromise); the latter indicates the bundle was bad on arrival.

### §21.8 Component in `RECOVERY_PENDING` reboot delay

The system in `RECOVERY_PENDING` waits for an operator-initiated reboot. During this wait, evidence emission continues normally (other failures observed during this window are still recorded), but no actions can be initiated. There is no automatic reboot timer that fires after N minutes — the reboot is operator-initiated. This is intentional: an automated reboot triggered by a transient failure could amplify the failure into a service outage.

### §21.9 Catastrophic recovery escalation cap

If `HALTED → recovery → HALTED → recovery → HALTED` happens within the §6.4 window, `RECOVERY_LOOP_DETECTED` fires. The runtime then halts at `STAGE_RECOVERY_SHELL_READY` with a flag that prevents the next normal-mode boot from being attempted automatically; only operator action can cross the loop-breaker.

### §21.10 Evidence retention vs failure floods

The retention policies (STANDARD_24M, EXTENDED_60M, FOREVER) are per S3.1. A failure flood of `FAILURE_OBSERVED` (STANDARD_24M) records is rate-limited (§9.5). FOREVER records (TAMPER_DETECTED, BUNDLE_REJECTED, BACKEND_VERSION_MISMATCH, COMPONENT_RESTART_BUDGET_EXHAUSTED, RECOVERY_LOOP_DETECTED, HALTED_PENDING_OPERATOR) are never rate-limited; they survive any retention pressure.

## §22 See also

- [L0 Constitutional Invariants (S6.4)](../L0_Governance_Evidence_Safety/04_invariants.md)
- [S9.1 Recovery Boundary](../L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md)
- [S2.3 Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S2.4 Verification Grammar](02_verification_grammar.md)
- [S3.1 Evidence Log](01_evidence_log.md)
- [S5.2 Vault Broker](../L4_Policy_Identity_Vault/02_vault_broker.md)
- [S8.1 Network Policy](../L8_Network_Hardware_Devices/02_network_policy.md)
- [S10.1 Capability Runtime gRPC](../L3_AIOS_SGR_Service_Graph_Runtime/03_capability_runtime_grpc.md)
- [S13.1 Cognitive Core Model](../L5_Cognitive_Core/01_cognitive_core_model.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

---

**Status:** REAL
**Evidence:** E1 (file exists; structural contract complete; closed enums declared; failure → behavior table closed; FSM closed; circuit-breaker discipline closed; runbook lookup contract closed; adversarial robustness covered; telemetry contract bounded; ten evidence record types queued for S3.1).
