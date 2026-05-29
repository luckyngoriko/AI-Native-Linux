# S16.7 — Service Hardening Score Gates

| Field     | Value                                                                                                                                                                                                                         |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-29; awaiting implementation evidence)                                                                                                                                                             |
| Phase tag | S16.7                                                                                                                                                                                                                         |
| Layer     | L3/L9 (with L0 status taxonomy, L4 policy gate consumption)                                                                                                                                                                   |
| Consumes  | S3.2 Sandbox Composition, S16.1 Security Profile Matrix, S16.3 STIG/NIST Control Map + Scanner (control `AIOS-CM-0003`), S3.1 Evidence Log, S15.1 SGR Unit Manifest                                                           |
| Produces  | `ServiceClass` enum, `ServiceHardeningRequirements`, `ServiceHardeningScore`, per-service-class score floors, the service promotion gate, `SERVICE_HARDENING_SCORED` / `SERVICE_PROMOTION_BLOCKED_LOW_SCORE` evidence records |

## 1. Responsibility

S16.7 makes the runtime hardening of long-running AIOS systemd services
**measurable, scored, and gated**. Rev.2 sandbox composition (S3.2) bounds the
blast radius of one-shot actions and app launches; it does not assign a standing
exposure score to the persistent service units that _implement_ AIOS — the
Policy Kernel daemon, the Evidence Log writer, the Vault Broker, the Capability
Runtime, renderers, schedulers, and adapters. Planning finding `SEC-007`
("service hardening is not scored") names this gap exactly.

S16.7 closes it by defining:

- per-service-class **systemd unit hardening requirements** (mandatory and
  forbidden unit directives);
- a `ServiceHardeningScore` modeled on `systemd-analyze security` (a 0.0–10.0
  exposure level with named sub-checks);
- per-`ServiceClass` **score floors** (the worst exposure a class may carry);
- the **promotion gate** that blocks a service from reaching active state in
  `STIG_ALIGNED`/`AIRGAP_HIGH` when its measured score is worse than its floor;
- the score floors that S16.3 control `AIOS-CM-0003` references (that control is
  currently a dangling reference to "profile hardening score floors"; S16.7 is
  where those floors are defined).

S16.7 does **not** redefine the unit lifecycle, sandbox primitives, or the MAC
plane. It scores what S3.2/S3 already model and feeds the verdict into the S16.3
scanner and S16.1 profile gate.

Invariant links: INV-002, INV-004, INV-005, INV-008, INV-012, INV-013, INV-014,
INV-017, INV-018, INV-025.

## 2. Product principle

A service is allowed to run with broad privilege only if someone proved it
_needs_ that privilege and recorded the proof. The default posture is "minimal
exposure, scored, and gated," not "works on my machine, ship it."

```text
service unit defined
  -> derive ServiceClass
  -> measure ServiceHardeningScore (named sub-checks)
  -> compare against class floor for active SecurityProfile
  -> emit SERVICE_HARDENING_SCORED evidence
  -> at or below floor: allow promotion to active
  -> worse than floor: block, emit SERVICE_PROMOTION_BLOCKED_LOW_SCORE, show fix
```

The operator never has to read `systemd-analyze security` output to know whether
a service is safe to promote. The score, the failing sub-checks, the exact
directive to add, and the blocked reason are surfaced as state, not logs.

## 3. Reference patterns

| Pattern                                                                                                                                               | S16.7 use                                                                                                                                                                |
| ----------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| [`systemd-analyze security`](https://www.freedesktop.org/software/systemd/man/latest/systemd-analyze.html#systemd-analyze%20security%20UNIT%E2%80%A6) | Authoritative model for the exposure score (0.0 safest → 10.0 most exposed) and the named per-directive sub-checks.                                                      |
| [systemd sandboxing directives](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html)                                            | `NoNewPrivileges`, `ProtectSystem`, `PrivateTmp`, `CapabilityBoundingSet`, `SystemCallFilter`, `RestrictAddressFamilies`, etc. — the directives the sub-checks evaluate. |
| [systemd `CapabilityBoundingSet`](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html#Capabilities)                             | Bounding-set reduction model for the capability sub-check.                                                                                                               |
| [seccomp / `SystemCallFilter`](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html#System%20Call%20Filtering)                   | Allowlist-style syscall filtering and `@system-service` set used by the syscall sub-check.                                                                               |
| [DISA STIG service hardening rules](https://public.cyber.mil/stigs/)                                                                                  | External hardening-target source bound through S16.3 `external_refs.DISA_STIG`.                                                                                          |
| [NIST SP 800-53 CM-7 Least Functionality](https://csrc.nist.gov/pubs/sp/800/53/r5/upd1/final)                                                         | Control-family anchor for "only required functions/ports/privileges enabled."                                                                                            |

The scoring math is AIOS-native and deterministic (Section 6). It is _modeled on_
`systemd-analyze security` so operators can cross-check, but the AIOS score is
the internal source of truth, exactly as S16.3 treats OpenSCAP as an export
target rather than the authority.

## 4. Service classes

Every AIOS-owned systemd unit is assigned exactly one `ServiceClass`. The class
sets the hardening floor and the mandatory/forbidden directive set.

```text
ServiceClass =
  CONSTITUTIONAL_CORE
| SECURITY_BROKER
| CAPABILITY_RUNTIME
| AI_PLANE
| RENDERER_SURFACE
| OBSERVABILITY
| SYSTEM_INTEGRATION
| RECOVERY_SERVICE
| DEV_FIXTURE
```

Unknown values are rejected by the `ServiceHardeningRequirements` loader.

| Class                 | Example units                                             | Posture intent                                                             |
| --------------------- | --------------------------------------------------------- | -------------------------------------------------------------------------- |
| `CONSTITUTIONAL_CORE` | Policy Kernel daemon, Evidence Log writer                 | Most-hardened; touches constitutional truth; must never be `unconfined_t`. |
| `SECURITY_BROKER`     | Vault Broker, key/crypto broker                           | Owns secret material; strictest memory and filesystem isolation.           |
| `CAPABILITY_RUNTIME`  | Capability Runtime, typed-action executor, adapters       | Executes typed actions; broad-but-bounded; per-adapter sub-scoping.        |
| `AI_PLANE`            | Native AI Control Plane, intent interpreter, model router | AI-facing; no privilege escalation, no eBPF authorship (INV-025).          |
| `RENDERER_SURFACE`    | Web renderer, KDE bridge, voice/STT-TTS service           | Untrusted-input-facing; network and device exposure scored hard.           |
| `OBSERVABILITY`       | Telemetry collector, scanner runner, log shipper          | Read-mostly; may need broad read but no write authority over evidence.     |
| `SYSTEM_INTEGRATION`  | `aios-system` orchestrator, schedulers                    | Coordinates other services; no direct mutation authority.                  |
| `RECOVERY_SERVICE`    | Recovery-path units                                       | Must boot without the Cognitive Core (INV-001); minimal, signed.           |
| `DEV_FIXTURE`         | Dev-only test/mock units                                  | Permitted relaxed score **only** under `DEV_RELAXED`; forbidden elsewhere. |

A unit's class is declared in its S3 unit manifest and is itself an evidenced,
policy-checked attribute; an AI subject cannot reclassify a unit to obtain a
weaker floor.

## 5. Service hardening requirements

`ServiceHardeningRequirements` is the per-class contract that every unit of that
class must satisfy. It is loaded from signed data; unknown keys, unknown enum
values, and unknown directive names are rejected by the loader.

```yaml
service_hardening_requirements:
  schema: "aios.s16.service_hardening_requirements/1"
  service_class: CONSTITUTIONAL_CORE
  # directives that MUST be present with at least the stated value
  mandatory_directives:
    NoNewPrivileges: "true"
    ProtectSystem: "strict" # off|true|full|strict ; strict = most isolated
    ProtectHome: "true"
    PrivateTmp: "true"
    PrivateDevices: "true"
    ProtectKernelTunables: "true"
    ProtectKernelModules: "true"
    ProtectKernelLogs: "true"
    ProtectControlGroups: "true"
    RestrictNamespaces: "true"
    RestrictRealtime: "true"
    RestrictSUIDSGID: "true"
    LockPersonality: "true"
    MemoryDenyWriteExecute: "true"
    SystemCallFilter: "@system-service" # allowlist baseline
    SystemCallArchitectures: "native"
    CapabilityBoundingSet: "" # empty = drop all unless explicitly granted
    RestrictAddressFamilies: "AF_UNIX" # no AF_INET/AF_INET6 unless granted
  # directives that MUST NOT appear (or must equal the forbidden-negation)
  forbidden_directives:
    - "PrivilegedTrue" # PrivilegeEscalation / no privileged mode
    - "CapabilityBoundingSet=~" # negated (grant-all) bounding set
    - "SystemCallFilter=@privileged"
    - "DeviceAllow=*"
    - "BindReadWritePaths=/" # write access to root
  # MAC requirement; ties to S16.2 SELinux MAC plane
  selinux:
    require_confined_domain: true # MUST run in an AIOS domain
    forbid_unconfined_t: true # hard: AIOS-owned services never unconfined_t
  # capability grants must be itemized; default is none
  capability_grants_allowed: [] # e.g. [CAP_NET_BIND_SERVICE] only with reason
```

Two cross-cutting hard rules apply to **every** class except `DEV_FIXTURE`:

1. `forbid_unconfined_t: true` — an AIOS-owned service started in
   `unconfined_t` is a hard deny. This is the runtime-service form of the S16.1
   `hd.s16.unconfined_aios_service` deny and the S16.2 MAC rule that policy,
   evidence, vault, recovery, and agent domains never run unconfined. It applies
   in **all** profiles, including `DEV_RELAXED` (a dev fixture may be unconfined;
   a real AIOS service may not).
2. `MemoryDenyWriteExecute: "true"` and a non-grant-all `SystemCallFilter` are
   mandatory for `CONSTITUTIONAL_CORE`, `SECURITY_BROKER`, and `AI_PLANE`. The
   AI plane additionally must hold no capability that permits BPF program load
   (`CAP_BPF`, `CAP_SYS_ADMIN`), enforcing INV-025 ("AI cannot author eBPF") at
   the unit level.

## 6. ServiceHardeningScore schema

The score is computed deterministically from the unit's effective directives
against the class requirements. The numeric exposure scale matches
`systemd-analyze security`: **0.0 is the safest, 10.0 is the most exposed.**
Each sub-check contributes a weighted exposure amount; the total is the sum of
unmitigated sub-check weights, clamped to `[0.0, 10.0]`.

```yaml
service_hardening_score:
  schema: "aios.s16.service_hardening_score/1"
  unit: "aios-policy-kernel.service"
  service_class: CONSTITUTIONAL_CORE
  profile_id: STIG_ALIGNED
  # 0.0 safest .. 10.0 most exposed (lower is better)
  exposure_score: 1.4
  overall_rating: HARDENED # see rating bands below
  sub_checks:
    - id: NoNewPrivileges
      directive: "NoNewPrivileges"
      observed: "true"
      satisfied: true
      weight: 1.0
      exposure_contribution: 0.0
    - id: ProtectSystem
      directive: "ProtectSystem"
      observed: "strict"
      satisfied: true
      weight: 0.8
      exposure_contribution: 0.0
    - id: PrivateTmp
      directive: "PrivateTmp"
      observed: "true"
      satisfied: true
      weight: 0.4
      exposure_contribution: 0.0
    - id: CapabilityBoundingSet
      directive: "CapabilityBoundingSet"
      observed: ""
      satisfied: true
      weight: 1.2
      exposure_contribution: 0.0
    - id: SystemCallFilter
      directive: "SystemCallFilter"
      observed: "@system-service"
      satisfied: true
      weight: 1.0
      exposure_contribution: 0.0
    - id: RestrictAddressFamilies
      directive: "RestrictAddressFamilies"
      observed: "AF_UNIX"
      satisfied: true
      weight: 0.6
      exposure_contribution: 0.0
    - id: MemoryDenyWriteExecute
      directive: "MemoryDenyWriteExecute"
      observed: "true"
      satisfied: true
      weight: 0.8
      exposure_contribution: 0.0
    - id: SELinuxConfinement
      directive: "selinux.domain"
      observed: "aios_policy_t"
      satisfied: true
      weight: 2.0
      exposure_contribution: 0.0
  unsatisfied_sub_checks: [] # ids of sub_checks where satisfied=false
  measured_at: "2026-05-29T00:00:00Z"
  evidence_receipt_id: "evr_..."
```

Closed `overall_rating` enum (derived from `exposure_score`, lower is safer):

```text
ServiceHardeningRating =
  HARDENED          # exposure_score <= 2.0
| ACCEPTABLE        # 2.0 <  exposure_score <= 4.0
| MEDIUM_EXPOSURE   # 4.0 <  exposure_score <= 6.0
| HIGH_EXPOSURE     # 6.0 <  exposure_score <= 8.0
| DANGEROUS         # exposure_score > 8.0
```

Unknown values are rejected by the score validator. The bands match the
`systemd-analyze security` rating ladder ("OK"→"UNSAFE") so an operator can
cross-read, but the AIOS rating is authoritative.

The closed sub-check id set (each id maps to one named directive family):

```text
ServiceHardeningSubCheck =
  NoNewPrivileges
| ProtectSystem
| ProtectHome
| PrivateTmp
| PrivateDevices
| ProtectKernelTunables
| ProtectKernelModules
| ProtectKernelLogs
| ProtectControlGroups
| RestrictNamespaces
| RestrictRealtime
| RestrictSUIDSGID
| LockPersonality
| MemoryDenyWriteExecute
| CapabilityBoundingSet
| SystemCallFilter
| SystemCallArchitectures
| RestrictAddressFamilies
| SELinuxConfinement
```

Unknown sub-check ids are rejected by the score validator. The scorer is
deterministic: the same effective unit + same requirements always yields the
same `exposure_score`.

## 7. Per-service-class score floors

A **floor** is the worst (highest) `exposure_score` a class may carry and still
be promotable. A unit passes the gate when `exposure_score <= floor` for the
active profile. Stricter profiles tighten the floor; they never loosen it.

| ServiceClass          | `DEV_RELAXED` | `SECURE_DEFAULT` | `STIG_ALIGNED` | `AIRGAP_HIGH` |
| --------------------- | ------------- | ---------------- | -------------- | ------------- |
| `CONSTITUTIONAL_CORE` | 3.0           | 2.5              | 2.0            | 1.5           |
| `SECURITY_BROKER`     | 3.0           | 2.5              | 2.0            | 1.5           |
| `RECOVERY_SERVICE`    | 3.5           | 3.0              | 2.5            | 2.0           |
| `AI_PLANE`            | 4.0           | 3.5              | 3.0            | 2.5           |
| `CAPABILITY_RUNTIME`  | 4.5           | 4.0              | 3.5            | 3.0           |
| `SYSTEM_INTEGRATION`  | 4.5           | 4.0              | 3.5            | 3.0           |
| `OBSERVABILITY`       | 5.0           | 4.5              | 4.0            | 3.5           |
| `RENDERER_SURFACE`    | 5.0           | 4.5              | 4.0            | 3.5           |
| `DEV_FIXTURE`         | 8.0           | n/a              | n/a            | n/a           |

Floor rules:

1. `DEV_FIXTURE` exists **only** under `DEV_RELAXED`. A `DEV_FIXTURE`-class unit
   present under `SECURE_DEFAULT`, `STIG_ALIGNED`, or `AIRGAP_HIGH` is a hard
   deny (`n/a` cells), independent of its score.
2. Floors are part of the signed profile data and rejected by the loader if a
   class is missing a floor for a profile that admits it.
3. The `forbid_unconfined_t` rule and any `forbidden_directives` match are
   structural hard denies that block promotion **regardless of numeric score**.
   The numeric floor is a _necessary_ condition, never a _sufficient_ one.
4. An AI subject can neither set, edit, nor request relaxation of a floor.
   Floor changes follow the S16.1 profile-transition gate (human approval +
   downgrade evidence).

## 8. Promotion gate

The gate is invoked when a service is about to transition into active/running
state (S3 unit lifecycle) on a host. It composes with — and never replaces —
the S3.2 sandbox composition that already bounds the unit.

```text
service activation requested
  -> resolve ServiceClass from S3 unit manifest
  -> read active SecurityProfile (S16.1)
  -> compute ServiceHardeningScore (Section 6)
  -> structural checks (forbidden directives, forbid_unconfined_t, DEV_FIXTURE-in-strict)
  -> numeric check: exposure_score <= class floor for profile
  -> emit SERVICE_HARDENING_SCORED (always)
  -> PASS  : allow activation; scanner records AIOS-CM-0003 status
  -> FAIL  : block activation; emit SERVICE_PROMOTION_BLOCKED_LOW_SCORE; show fix
```

Gate verdict by profile:

| Profile          | On score worse than floor / structural fail                                                           |
| ---------------- | ----------------------------------------------------------------------------------------------------- |
| `DEV_RELAXED`    | Warn, allow with `SERVICE_HARDENING_SCORED` (status `WARN`); never auto-fix.                          |
| `SECURE_DEFAULT` | Warn on numeric miss; **block** on structural hard deny (`forbid_unconfined_t`, forbidden directive). |
| `STIG_ALIGNED`   | **Block** on any miss (numeric or structural); emit `SERVICE_PROMOTION_BLOCKED_LOW_SCORE`.            |
| `AIRGAP_HIGH`    | **Block** on any miss; no exception may be requested live (S16.1 airgap rule).                        |

Under `STIG_ALIGNED`, a blocked promotion may proceed only via an S16.3
exception register entry (expiring, human-owned, compensating control, AI
cannot approve). Under `AIRGAP_HIGH`, no live exception is permitted; the unit
must be hardened or removed.

This gate is the concrete enforcement behind S16.3 control `AIOS-CM-0003`
("systemd services meet profile hardening score floors"). The S16.3 scanner's
`Service posture` probe class reads `SERVICE_HARDENING_SCORED` evidence to set
that control's `HardeningCheckResult.status`.

## 9. Security profile gates

| Profile          | Service hardening posture                                                                                                                 |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `DEV_RELAXED`    | Scoring runs and is evidenced; numeric misses warn; `DEV_FIXTURE` permitted; `forbid_unconfined_t` still enforced for real AIOS services. |
| `SECURE_DEFAULT` | Floors enforced as warnings on numeric miss; structural hard denies block; no `DEV_FIXTURE`.                                              |
| `STIG_ALIGNED`   | Floors are hard gates; any miss blocks promotion; exceptions only via S16.3 register (expiring, human, AI-cannot-approve).                |
| `AIRGAP_HIGH`    | Tightest floors; any miss blocks; no live exception; offline evidence export of every `SERVICE_HARDENING_SCORED`.                         |

Hard denies (all profiles unless noted):

- No AIOS-owned service may run in `unconfined_t` (`DEV_FIXTURE` exempt only
  under `DEV_RELAXED`).
- No service may be promoted to active under `STIG_ALIGNED`/`AIRGAP_HIGH` with
  `exposure_score` worse than its class floor.
- No AI subject may set, edit, relax, or approve a floor, a class assignment, a
  forbidden-directive waiver, or a service hardening exception.
- No service may grant `CAP_BPF`/`CAP_SYS_ADMIN` to an `AI_PLANE`-class unit
  (preserves INV-025 at the unit boundary).
- No `DEV_FIXTURE`-class unit may exist outside `DEV_RELAXED`.

## 10. Non-goals

- Do not re-implement `systemd-analyze security`; AIOS computes its own
  deterministic score and uses systemd's tool only as a cross-check/reference.
- Do not redefine the S3 unit lifecycle, the S3.2 sandbox primitives, or the
  S16.2 SELinux MAC plane; S16.7 scores and gates them.
- Do not score third-party/foreign app capsules here — those are bounded by S17
  app capsule isolation, not by AIOS service-class floors.
- Do not promise that a high score implies functional correctness; the score
  measures exposure, not behavior.
- Do not let a numeric pass override a structural hard deny.
- Do not allow score tuning to weaken a profile floor for convenience.

## 11. Evidence records

S16.7 adds these evidence record types:

```text
SERVICE_HARDENING_SCORED
SERVICE_PROMOTION_BLOCKED_LOW_SCORE
```

`SERVICE_HARDENING_SCORED` minimum fields:

```text
unit
service_class
profile_id
exposure_score
overall_rating
floor_applied
gate_verdict: PASS | WARN | FAIL
unsatisfied_sub_checks
structural_denies
measured_at
evidence_receipt_id
```

`SERVICE_PROMOTION_BLOCKED_LOW_SCORE` minimum fields:

```text
unit
service_class
profile_id
exposure_score
floor_applied
failing_sub_checks
structural_denies
remediation_hint
exception_id
evidence_receipt_id
blocked_at
```

Both record types are appended to the S3.1 Evidence Log and are append-only; an
AI subject cannot author, edit, or suppress them (INV-014).

## 12. Acceptance criteria

S16.7 is `REAL` only when:

1. `ServiceClass`, `ServiceHardeningRating`, and `ServiceHardeningSubCheck`
   enums are closed and unknown values are rejected by the loader/validator.
2. `ServiceHardeningRequirements` loads from signed data and rejects unknown
   directive names, unknown keys, and unknown enum values.
3. The scorer is deterministic: the same effective unit and requirements yield
   the same `exposure_score` on repeated runs.
4. Every AIOS-owned systemd unit resolves to exactly one `ServiceClass` and a
   floor exists for it in every profile that admits the class.
5. An AIOS-owned service in `unconfined_t` is blocked in all profiles
   (`DEV_FIXTURE` exempt only under `DEV_RELAXED`), independent of numeric score.
6. Under `STIG_ALIGNED`, a unit whose `exposure_score` is worse than its class
   floor is blocked from activation and emits
   `SERVICE_PROMOTION_BLOCKED_LOW_SCORE`.
7. Every gate evaluation emits `SERVICE_HARDENING_SCORED` with the verdict, the
   floor applied, and the unsatisfied sub-checks.
8. The S16.3 scanner sets `AIOS-CM-0003` status from `SERVICE_HARDENING_SCORED`
   evidence (the dangling floor reference is now resolved).
9. A `DEV_FIXTURE`-class unit present under any profile other than
   `DEV_RELAXED` is a hard deny.
10. No AI subject can set, edit, relax, or approve a floor, class, directive
    waiver, or hardening exception; such attempts are denied and evidenced.

## 13. See also

- [S16 Security Hardening and Compliance](00_overview.md)
- [S16.1 Security Profile Matrix](01_security_profile_matrix.md)
- [S16.2 SELinux MAC Policy Plane](02_selinux_mac_policy_plane.md)
- [S16.3 STIG/NIST Control Map + Scanner](03_stig_nist_control_map_scanner.md)
- [S3.2 Sandbox Composition](../../002.AI-OS.NET--SPECREV.2/L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [S3 Unit Manifest](../../002.AI-OS.NET--SPECREV.2/L3_AIOS_SGR_Service_Graph_Runtime/01_unit_manifest.md)
- [S3.1 Evidence Log](../../002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/01_evidence_log.md)
- [Rev.3 Planning Notes (SEC-007)](../00_PLANNING_NOTES.md)
