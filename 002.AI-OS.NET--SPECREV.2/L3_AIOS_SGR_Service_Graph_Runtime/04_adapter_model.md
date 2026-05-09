# Adapter Model — Manifest, Registration, Capability Declaration, Fail-Closed Semantics (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists; structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Phase tag      | S15.3                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| Layer          | L3 AIOS-SGR / Capability Runtime                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Schema package | `aios.adapter.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| Consumes       | S10.1 Capability Runtime gRPC (the `AdapterManifest` skeleton, `ActionDispatchKind`, `AdapterIOMode`, `AdapterStability`, `ExecutionFailureReason`); S0.1 Action Envelope + Lifecycle; S3.2 Sandbox Composition (`SandboxProfile`, runtime safety floor); S11.1 Repository Model (publisher trust chain, `PackageKind = ADAPTER`, Ed25519 signature discipline); S2.4 Verification Engine; S2.3 Policy Kernel; L4.2 Vault Broker; L0 invariant catalog (INV-002, INV-008, INV-013, INV-014, INV-015, INV-017) |
| Produces       | the closed `aios.adapter.v1alpha1.AdapterManifest` deepening of S10.1's skeleton; the closed `AdapterRegistrationState` FSM; the closed `AdapterCapabilityClass` (ten values), `AdapterIOMode`, `AdapterDispatchKind`, `AdapterFailureMode` enums; the registration / hot-reload / deregistration discipline; ten evidence record types queued for S3.1; per-dispatch-kind performance budgets; three worked examples                                                                                         |

## §1 Purpose

This sub-spec defines the **adapter contract** that sits underneath the L3 Capability Runtime. S10.1 (`03_capability_runtime_grpc.md`) already specifies an `AdapterManifest` skeleton inside the `aios.runtime.v1alpha1` package — the orchestration layer that consumes adapters. This file does **not** redefine that skeleton. It **deepens** it into the full, sealed adapter contract: the manifest schema, the registration finite-state machine, the capability declaration vocabulary, the lifecycle of an adapter from `DRAFT` to `RETIRED`, the fail-closed semantics that bind every dispatch path, and the adversarial-robustness rules that hold against forged signatures, replay attacks, capability lies, and runtime kind-overrun.

The S10.1 manifest is the wire surface that the runtime consumes. The S15.3 manifest is what an adapter **author** writes, what an operator **registers**, and what the trust chain **signs**. The two surfaces are layered:

```text
S15.3 author-facing AdapterManifest (aios.adapter.v1alpha1.AdapterManifest)
            |
            v  (registration via runtime.adapter.register typed action)
S10.1 runtime-facing AdapterManifest (aios.runtime.v1alpha1.AdapterManifest)
            |
            v
adapter dispatch under chosen ActionDispatchKind + composed SandboxProfile
```

The S10.1 wire surface is the **subset** the runtime needs at dispatch time: id, declared actions, dispatch kind, IO mode, stability, default sandbox profile id, signature. The S15.3 surface adds the full publisher trust chain, the closed `AdapterCapabilityClass` declaration set, the sandbox-profile **floor** the manifest commits to, the supported-invariants list, the failure-mode declarations, and the registration FSM with hot-reload / kind-overrun / capability-overrun semantics.

This file defines:

1. The full `AdapterManifest` record, deepening the S10.1 skeleton.
2. The closed `AdapterRegistrationState` FSM (six states) and the allowed-transition table.
3. The closed `AdapterCapabilityClass` enum (ten values).
4. The closed `AdapterIOMode` and `AdapterDispatchKind` enums (mirroring S10.1).
5. The closed `AdapterFailureMode` enum (ten values, mirroring S10.1's `ExecutionFailureReason` superset).
6. The registration discipline: signature verification, action-kind catalog cross-check, capability grant cross-check, manifest sealing.
7. The hot-reload contract: versioned manifest updates with seamless action draining.
8. The adversarial-robustness section: forged signatures, kind-overrun, capability-overrun, manifest replay (downgrade attacks).
9. The performance contract per dispatch kind.
10. Ten evidence record types queued for S3.1.
11. Three worked examples (filesystem, networking, GPU adapters).

This file does **not** define:

- The S10.1 orchestration RPCs (`ValidateAction`, `ExecuteAction`, etc.). That is S10.1.
- The pre-dispatch eight-step sequence. That is S10.1 §6.
- Adapter implementation patterns, per-action target schemas, per-action verification probes. Those are author-side concerns, not contract-shaped.
- The S11.1 publisher onboarding flow, repository fetch, or distribution mirror semantics. That is S11.1.
- The S3.2 sandbox composition algorithm or runtime safety floor itself. That is S3.2.
- The S2.4 verification grammar. That is S2.4.

## §2 Scope

In scope:

- The closed `aios.adapter.v1alpha1.AdapterManifest` record and its sub-messages.
- The closed `AdapterRegistrationState`, `AdapterCapabilityClass`, `AdapterIOMode`, `AdapterDispatchKind`, `AdapterFailureMode` enums.
- The registration FSM, including allowed and forbidden transitions.
- The trust chain (AIOS root → `VERIFIED` publisher → adapter signing key) and Ed25519 signature discipline that admits an adapter.
- The action-kind exclusivity and L5 capability catalog cross-check at registration.
- The capability-grant cross-check at registration (an adapter cannot declare `VAULT_OPERATION` if its registering subject does not hold the corresponding L4 grant capacity).
- The sandbox profile **minimum** binding: the manifest commits to a floor it cannot run weaker than; any composition that would drop below the manifest minimum forces dispatch refusal.
- The hot-reload pathway: in-flight actions complete on the old manifest; new submissions resolve under the new manifest.
- The deregistration pathway: voluntary, expiry-driven, kind-overrun-driven, capability-overrun-driven, operator-driven.
- The adversarial-robustness section.
- Per-dispatch-kind performance budgets (registration check at p95).
- The closed list of ten evidence record types this sub-spec adds.
- Three worked examples.

Out of scope (referenced, not redefined):

- The action envelope. (S0.1.)
- The orchestration RPCs and the pre-dispatch sequence. (S10.1.)
- Verification grammar. (S2.4.)
- Sandbox composition algorithm. (S3.2.)
- Publisher onboarding, repository fetch, mirror discipline. (S11.1.)
- Capability translator, capability catalog content. (S1.1, L5.)

### §2.1 Reading order

A reader who already understands S10.1 can skip §3.3 and §3.4 (mirror enums) and read §3.1, §3.2, §3.5, §4 (full manifest), §5 (FSM), §6 (admission), §7 (lifecycle changes), §10 (adversarial), §12 (examples) — that is the deepening proper. A reader new to L3 should read S10.1 first; the orchestration context is load-bearing.

### §2.2 Reading dependencies

This sub-spec assumes familiarity with: S10.1's `ActionLifecycleState` FSM and pre-dispatch eight steps; S0.1's `ActionEnvelope` shape and `idempotency_key`/`request_hash` discipline; S3.2's `SandboxProfile` shape and runtime safety floor concept; S11.1's publisher trust chain. Where this sub-spec needs a definition from one of those, the reference is explicit; nothing is invented locally.

## §3 Closed enums

### §3.1 `AdapterRegistrationState`

Closed enum. Six states. The state of an adapter inside the runtime's adapter directory; orthogonal to but interlocked with `AdapterStability` (S10.1 §3.4).

| Value          | Semantics                                                                                                                                                                                                                                                                                         |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `DRAFT`        | A `runtime.adapter.register` typed action has been submitted with a candidate manifest; the manifest is undergoing pre-validation but has not yet entered signature verification. No dispatches accepted. Visible in `ListAdapters` only with explicit `include_drafts=true`.                     |
| `VALIDATING`   | The manifest passed pre-validation; signature verification, publisher trust chain check, action-kind catalog cross-check, and capability-grant cross-check are in flight (steps §6.2.1 — §6.2.6). No dispatches accepted.                                                                         |
| `REGISTERED`   | The manifest passed all admission checks and is sealed in the adapter directory. The adapter accepts dispatches under the runtime's S10.1 §6 eight-step pre-dispatch sequence. The default operating stability is `REGISTERED` (per S10.1 §3.4) until an operator promotes via `set_stability`.   |
| `DEGRADED`     | The adapter is registered but a runtime health signal (timeout rate, panic rate, kind-overrun, capability-overrun preliminary observation) has crossed a threshold. Dispatches still accepted but routed behind the degradation backpressure budget (S10.1 §11). Auto-clears or escalates per §8. |
| `DEREGISTERED` | The adapter was removed from the directory (voluntary, expiry, kind-overrun, capability-overrun, operator action). New dispatches are rejected with `UNKNOWN_ACTION_KIND`. In-flight actions on the adapter complete on their existing manifest snapshot (no mid-action manifest swap).           |
| `RETIRED`      | Terminal forensic state. The adapter id is permanently retired; no manifest with the same `adapter_id` and a non-strictly-greater version may be re-registered. `ListAdapters` still returns the adapter for forensic reasons. Action submissions referencing it fail with `UnknownAdapter`.      |

The six-state set is sealed. `RETIRED` is deliberately distinct from `DEREGISTERED`: `DEREGISTERED` is a clean withdrawal where the same `adapter_id` may be re-registered with a higher version; `RETIRED` is a constitutional ban — it is reached only by kind-overrun, capability-overrun, or signature forgery, and it forecloses re-admission of that adapter id. Conflating the two would erase a forensically significant distinction.

### §3.2 `AdapterCapabilityClass`

Closed enum. Ten values. Every adapter manifest must enumerate the closed set of capability classes it requires; the runtime cross-checks each declared class against the registering subject's L4 grant capacity (§6.2.5) at registration and against the policy decision (§6.3) at dispatch. Capabilities not declared in the manifest **cannot be invoked** at dispatch time — the adapter fails closed at the L4 policy gate even if the underlying syscall would succeed. This is the constitutional capability-honesty contract: an adapter may not lie about what it does.

| Value                  | Semantics                                                                                                                                                                                                                                           | Required publisher trust (minimum, S11.1) | Approval class hint                          |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------- | -------------------------------------------- |
| `FILESYSTEM_WRITE`     | Adapter mutates filesystem state — `/aios` (AIOS-FS object writes per L2) or, with stricter narrowing, host-bounded paths declared in `host_paths`. Always accompanied by a `SandboxProfile` mount-discipline minimum.                              | `VERIFIED`                                | `REQUIRE_APPROVAL` for AI-origin             |
| `FILESYSTEM_READ`      | Adapter reads filesystem state. Read access is a capability because the action subject's identity must be cross-checked against L2 namespace ACLs (S2 §03 namespace).                                                                               | `COMMUNITY`                               | `ALLOW` for human, `REQUIRE_APPROVAL` for AI |
| `SERVICE_LIFECYCLE`    | Adapter starts, stops, restarts, reloads, or otherwise transitions a desired-state SGR unit (per L3 `01_unit_manifest.md` once landed). Includes systemd, AIOS-SGR units, container lifecycle.                                                      | `VERIFIED`                                | `REQUIRE_APPROVAL` always                    |
| `NETWORK_OUTBOUND`     | Adapter initiates outbound network connections. Cross-checked against the manifest's `network_outbound_manifest` (S8.1) at composition time. AI-origin actions targeting this capability force `ISOLATED_SANDBOX` regardless of stability.          | `VERIFIED`                                | `REQUIRE_APPROVAL` for AI                    |
| `VAULT_OPERATION`      | Adapter requests Vault Broker operations (`KEY_SIGN`, `KEY_VERIFY`, `KEY_WRAP`, `KEY_UNWRAP`, `SECRET_USE` per L4.2). The adapter never receives raw secret material; INV-015 binds. Operations are mediated by the Vault Broker's typed surface.   | `VERIFIED`                                | `REQUIRE_APPROVAL` always                    |
| `GPU_RENDER`           | Adapter performs GPU rendering work — display-targeted compositor surfaces, video decode, scanout. Bound by L7 surface composition rules; cannot be combined with `GPU_COMPUTE` in the same manifest.                                               | `VERIFIED`                                | `ALLOW` for human, `REQUIRE_APPROVAL` for AI |
| `GPU_COMPUTE`          | Adapter performs GPU general-purpose compute (GPGPU) — model inference, tensor ops, hash workloads. Requires the explicit `gpu.compute_heavy` capability grant (L0 INV-024). Mutually exclusive with `GPU_RENDER` in the same manifest.             | `VERIFIED`                                | `REQUIRE_APPROVAL` always                    |
| `EXTERNAL_API_CALL`    | Adapter calls an external (non-AIOS) HTTPS API. The set of allowed hosts is enumerated in `external_api_hosts` and cross-checked against the network outbound manifest. Free-form egress is not a value — adapters must enumerate.                  | `VERIFIED`                                | `REQUIRE_APPROVAL` for AI                    |
| `EVIDENCE_EMIT`        | Adapter is authorised to emit evidence records of specific declared types (a closed sub-list per adapter). Default is empty; adapters typically rely on the runtime to emit on their behalf. Adapter-emitted evidence carries an attestation chain. | `AIOS_ROOT`                               | `REQUIRE_APPROVAL` always                    |
| `SCHEDULER_PRIVILEGED` | Adapter requests scheduler-privileged operations (real-time priority, cgroup tweaks beyond the sandbox profile, kernel scheduler tuning). Reserved for L1 substrate adapters and recovery-mode adapters; AI-origin actions hard-denied (INV-013).   | `AIOS_ROOT`                               | Recovery-mode operator only                  |

The ten-value set is sealed. There is no `OTHER` value, no extension class, no escape hatch. An adapter that needs a capability not in this list fails registration with `ADAPTER_REGISTRATION_REJECTED` and the manifest is preserved as a FOREVER forensic record. The constitutional posture: a missing capability class is a contract gap, not a runtime workaround.

Capability honesty is enforced at three points:

1. **At registration** — the manifest's `declared_capabilities` is cross-checked against the registering subject's L4 grant capacity (§6.2.5).
2. **At policy evaluation** — S2.3 evaluates the action's required capabilities (derived from the adapter manifest) against the subject's effective grants and the bundle constraints.
3. **At dispatch (defence in depth)** — the runtime composes the `SandboxProfile` such that capabilities **outside** `declared_capabilities` are syscall-denied at the kernel boundary; an adapter that exceeds its declared set hits an EPERM, not a privilege escalation, and the runtime emits `ADAPTER_CAPABILITY_VIOLATION` (§9, FOREVER) and forces `RETIRED`.

### §3.3 `AdapterIOMode`

Closed enum. Mirrors S10.1 §3.3 verbatim and is included here so the manifest is self-contained. Free-form shell command input is **not** a value (INV-013 binds; the L3 layer invariant binds; the S10.1 §3.3 commentary binds).

| Value                   | Semantics                                                                                                                                                                                                                                                                        |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `TYPED_PARAMETERS_ONLY` | Adapter accepts `request.target` as a typed proto/JSON struct validated against the adapter's per-action `target_schema`. Default mode and the strongly preferred mode for new adapters.                                                                                         |
| `TEMPLATE_PARAMETERS`   | Adapter accepts a closed template string with a closed set of named placeholders bound to typed values from `request.target`. Used for adapters that legitimately construct command lines (e.g. `pkg.install` shelling to `dnf`) without exposing free-form shell to the caller. |

A manifest declaring `TEMPLATE_PARAMETERS` whose template contains unbound `${...}` placeholders, free-form sub-command operators (`;`, `&&`, `||`, `|`, backticks, `$()`), or unbound redirections fails registration with `ADAPTER_REGISTRATION_REJECTED FOREVER`. The template author is required to specify the exact substitution-variable vocabulary; the runtime substitutes only those, in the quoting context the template author specified, against typed values. This contract must not be relaxed by any composition source — it binds INV-013 directly.

### §3.4 `AdapterDispatchKind`

Closed enum. Mirrors S10.1 §3.2's `ActionDispatchKind` (the manifest declares the **adapter's preferred** kind; the runtime's actual dispatch may upgrade to a stricter kind per the S10.1 §3.2 decision rule). Repeated here for the manifest's self-contained reading.

| Value              | Semantics                                                                                                                                                                                                                              |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `IN_PROCESS_RPC`   | Adapter handler runs inside the runtime's process. Reserved for low-latency, no-mutation adapters (status queries, manifest reads). Eligible only for `STABLE` adapters (S10.1 §3.2 enforces; §3.4 here describes).                    |
| `SUBPROCESS_FORK`  | Per-action subprocess. Default for filesystem mutation, service control, package operations on host-bounded adapters. Typical for production adapters.                                                                                 |
| `ISOLATED_SANDBOX` | Full sandbox per S3.2 `SandboxProfile`. Required for any AI-origin action; required for any action whose `risk.privileged` flag is true. The runtime upgrades into this kind at dispatch even if the manifest preferred a weaker kind. |
| `DRY_RUN`          | No mutation; adapter produces a simulation transcript only. Forced by `request.dry_run = SIMULATE` per S0.1 §9.3 and S10.1 §9.                                                                                                         |

The runtime's S10.1 §3.2 decision rule is authoritative. The manifest's preference is advisory; it cannot loosen the runtime's choice. An AI-origin action against an adapter that declared `IN_PROCESS_RPC` is upgraded to `ISOLATED_SANDBOX` at dispatch, and the dispatch path is unaffected — INV-002 binds.

### §3.5 `AdapterFailureMode`

Closed enum. Ten values. Mirrors and is a strict superset of the failure-mode subset of S10.1 §3.6 `ExecutionFailureReason` that is **adapter-attributable**. (S10.1's enum carries twelve values; the two non-adapter values — `IDEMPOTENCY_KEY_REPLAY_DETECTED` and `ENVELOPE_VALIDATION_FAILED` — are runtime-side and are not adapter-declarable.) The manifest must enumerate which failure modes the adapter can produce; the runtime uses this list to size timeout budgets, rollback strategies, and degradation thresholds.

| Value                          | Semantics                                                                                                                                            |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SANDBOX_APPLICATION_FAILED`   | The composed `SandboxProfile` could not be applied at dispatch (e.g. mount failure, cgroup creation refused, seccomp install failed). Pre-execution. |
| `ADAPTER_TIMEOUT`              | The adapter exceeded its declared per-action `timeout_seconds` (or the manifest default). Runtime interrupts.                                        |
| `ADAPTER_PANIC`                | The adapter process exited with a non-zero status, panicked, or crashed mid-execution.                                                               |
| `RESOURCE_BUDGET_EXCEEDED`     | The action's queue class budget, the per-subject rate limit, or the AI-share cap was exceeded at dispatch.                                           |
| `DEPENDENCY_UNREADY`           | A declared adapter dependency (systemd, AIOS-FS, network namespace) was not in a ready state at dispatch.                                            |
| `BACKEND_UNAVAILABLE`          | An external backend the adapter required (e.g. dnf metadata, AIOS-FS WAL, an external HTTPS endpoint) was unreachable.                               |
| `ROLLBACK_PRECONDITION_FAILED` | A rollback was requested but the adapter's declared `rollback_precondition` was not met.                                                             |
| `BINDING_EXPIRED`              | The held `ApprovalBinding` (S5.3) or `OverrideBinding` (S5.4) was expired or revoked at dispatch.                                                    |
| `ADAPTER_REFUSED`              | The adapter ran but explicitly refused the action (e.g. precondition assertion, declared invariant violation).                                       |
| `KIND_OR_CAPABILITY_OVERRUN`   | The adapter attempted to serve an action kind, or invoke a capability, outside its declared set. Constitutional violation; forces `RETIRED`.         |

The ten-value set is sealed. `KIND_OR_CAPABILITY_OVERRUN` is the constitutional break — the only failure mode that triggers `RETIRED` instead of `DEGRADED` or `DEREGISTERED`, because it represents a manifest lie (the adapter promised one set of behaviours and exhibited another). Manifests are not allowed to declare `KIND_OR_CAPABILITY_OVERRUN` as a tolerated failure mode (the field is informational; the runtime always treats it as terminal).

## §4 The full `AdapterManifest` record

### §4.1 Schema

```proto
syntax = "proto3";
package aios.adapter.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/struct.proto";

// Re-export of S10.1 enums for self-contained reading. The L3 implementation
// MUST reference the canonical S10.1 definitions; this re-declaration is
// schema-equivalent and identical-by-name. The build pipeline asserts
// equivalence at codegen.
import "aios/runtime/v1alpha1/runtime.proto";  // ActionDispatchKind, AdapterIOMode,
                                              // AdapterStability, RollbackStrategy
import "aios/sandbox/v1alpha1/sandbox_profile.proto";  // S3.2 SandboxProfile

// Closed; six values; §3.1.
enum AdapterRegistrationState {
  ADAPTER_REGISTRATION_STATE_UNSPECIFIED = 0;
  DRAFT                                  = 1;
  VALIDATING                             = 2;
  REGISTERED                             = 3;
  DEGRADED                               = 4;
  DEREGISTERED                           = 5;
  RETIRED                                = 6;
}

// Closed; ten values; §3.2.
enum AdapterCapabilityClass {
  ADAPTER_CAPABILITY_CLASS_UNSPECIFIED = 0;
  FILESYSTEM_WRITE                      = 1;
  FILESYSTEM_READ                       = 2;
  SERVICE_LIFECYCLE                     = 3;
  NETWORK_OUTBOUND                      = 4;
  VAULT_OPERATION                       = 5;
  GPU_RENDER                            = 6;
  GPU_COMPUTE                           = 7;
  EXTERNAL_API_CALL                     = 8;
  EVIDENCE_EMIT                         = 9;
  SCHEDULER_PRIVILEGED                  = 10;
}

// Closed; ten values; §3.5. `KIND_OR_CAPABILITY_OVERRUN` is informational
// in the manifest; the runtime always treats it as terminal-with-RETIRED.
enum AdapterFailureMode {
  ADAPTER_FAILURE_MODE_UNSPECIFIED = 0;
  SANDBOX_APPLICATION_FAILED       = 1;
  ADAPTER_TIMEOUT                  = 2;
  ADAPTER_PANIC                    = 3;
  RESOURCE_BUDGET_EXCEEDED         = 4;
  DEPENDENCY_UNREADY               = 5;
  BACKEND_UNAVAILABLE              = 6;
  ROLLBACK_PRECONDITION_FAILED     = 7;
  BINDING_EXPIRED                  = 8;
  ADAPTER_REFUSED                  = 9;
  KIND_OR_CAPABILITY_OVERRUN       = 10;
}

// The author-facing manifest. Deepens the S10.1 skeleton.
message AdapterManifest {
  // §4.2.1 — identity
  string adapter_id              = 1;   // "adapter:<vendor>:<name>:<version>"
  string vendor                  = 2;   // matches the adapter_id token
  string name                    = 3;   // matches the adapter_id token
  string adapter_version         = 4;   // SemVer; monotonic per adapter_id
  string spec_version            = 5;   // "v1alpha1"

  // §4.2.2 — declared actions and capabilities
  repeated AdapterActionDeclaration declared_actions = 6;
  repeated AdapterCapabilityClass declared_capabilities = 7;
  repeated string declared_invariants_supported = 8;  // L0 INV-XXX ids

  // §4.2.3 — dispatch and IO posture
  aios.runtime.v1alpha1.AdapterIOMode        io_mode = 9;
  aios.runtime.v1alpha1.ActionDispatchKind   preferred_dispatch_kind = 10;
  aios.runtime.v1alpha1.AdapterStability     declared_stability = 11;

  // §4.2.4 — sandbox floor
  // The minimum sandbox profile the adapter commits to. The composer (S3.2)
  // may compose to STRICTER; it MUST NOT compose to weaker. Composition that
  // would yield weaker than this floor causes dispatch refusal with
  // SANDBOX_APPLICATION_FAILED.
  aios.sandbox.v1alpha1.SandboxProfile sandbox_profile_minimum = 12;

  // §4.2.5 — declared failure modes
  repeated AdapterFailureMode declared_failure_modes = 13;

  // §4.2.6 — operational budgets
  uint32 default_adapter_timeout_seconds = 14;
  uint32 default_rollback_timeout_seconds = 15;

  // §4.2.7 — network / external API enumeration (paired with capability)
  repeated string network_outbound_hosts = 16;     // FQDNs or CIDRs (S8.1)
  repeated string external_api_hosts     = 17;     // FQDNs (HTTPS only)

  // §4.2.8 — evidence-emit declared types (only valid if declared_capabilities
  // contains EVIDENCE_EMIT; otherwise MUST be empty)
  repeated string declared_evidence_record_types = 18;

  // §4.2.9 — distribution provenance (S11.1 PackageKind = ADAPTER)
  string source_package_id            = 19;        // S11.1 package_id
  string publisher_root_id            = 20;        // S11.1 publisher catalog entry
  // S11.1 PublisherTrustLevel — must be VERIFIED or AIOS_ROOT for non-recovery
  // adapters; the trust check is enforced at registration (§6.2.4).

  // §4.2.10 — signature
  // Ed25519 over JCS of fields 1..20. Signed by an AIOS root key OR by a
  // VERIFIED publisher signing key whose chain terminates at the AIOS root
  // (S11.1 trust chain).
  bytes  manifest_signature = 21;
  string signing_key_id     = 22;
  google.protobuf.Timestamp manifest_created_at = 23;
  google.protobuf.Timestamp manifest_expires_at = 24;
}

message AdapterActionDeclaration {
  string action_kind                  = 1;   // dotted name; L5 capability catalog id
  google.protobuf.Struct target_schema   = 2;
  google.protobuf.Struct response_schema = 3;
  aios.runtime.v1alpha1.RollbackStrategy rollback_strategy = 4;
  uint32 timeout_seconds              = 5;   // overrides default; bounded
  string template_string              = 6;   // populated only when io_mode = TEMPLATE_PARAMETERS
  repeated string template_substitution_variables = 7;
  // Per-action capability overlay: this action requires a sub-set of the
  // manifest-declared capabilities. Capabilities listed here MUST appear in
  // the manifest-level declared_capabilities; capabilities outside that set
  // fail registration.
  repeated AdapterCapabilityClass per_action_capabilities = 8;
}
```

### §4.2 Field semantics

#### §4.2.1 Identity

`adapter_id` is `"adapter:<vendor>:<name>:<version>"` where `<version>` matches the manifest's `adapter_version` exactly. The three tokens are URL-safe (`[a-z0-9._-]+` for vendor and name; SemVer 2.0 for version). Inconsistency between the parsed `adapter_id` tokens and the explicit `vendor` / `name` / `adapter_version` fields is a registration failure (`ADAPTER_REGISTRATION_REJECTED FOREVER`) — the duplication is intentional defence in depth against a single-field mutation that would mismatch.

`adapter_version` is monotonic per `(vendor, name)`. The runtime tracks the highest version ever registered; a manifest with a strictly-older version (downgrade attack) is rejected with `ADAPTER_DOWNGRADE_REJECTED FOREVER` evidence (§9). Re-registering the same version is not a downgrade; it is a no-op (idempotent registration).

#### §4.2.2 Declared actions and capabilities

`declared_actions` lists the action kinds this adapter serves. Each kind must:

- exist in the L5 capability catalog (S1.1 §6.4) — registration cross-checks against the active catalog snapshot;
- not be currently owned by another `REGISTERED` adapter (action-kind exclusivity, S10.1 §10.5);
- not be in the catalog's deprecation set marked `RETIRED` — the catalog's own retirement closes the kind for new adapters.

`declared_capabilities` is a closed-set selection from `AdapterCapabilityClass`. Empty is allowed for read-only manifest-introspection adapters but is exceptional. Most adapters declare at least one of `FILESYSTEM_*` or `SERVICE_LIFECYCLE`. The set is the floor for the per-action `per_action_capabilities` overlays (per-action sets must be subsets of the manifest set).

`declared_invariants_supported` is the closed list of L0 INV-XXX ids the adapter respects. The runtime cross-checks at registration that the adapter does not declare support for invariants it would structurally violate (e.g. an adapter declaring `SCHEDULER_PRIVILEGED` cannot list `INV-013` — AI cannot perform system admin — as supported). Mis-declared invariant support fails registration.

#### §4.2.3 Dispatch and IO posture

`io_mode` and `preferred_dispatch_kind` are the manifest's preferences. The runtime's S10.1 §3.2 decision rule may upgrade `preferred_dispatch_kind` to a stricter kind at dispatch time; it never downgrades.

`declared_stability` is the **maximum** stability the adapter may claim. The actual operating stability is set by an operator through `runtime.adapter.set_stability` (S10.1 §3.4). A manifest that sets `declared_stability = STABLE` does not become `STABLE` at registration; it becomes `REGISTERED` and may be promoted later by an operator-class subject.

#### §4.2.4 Sandbox profile minimum (the floor binding)

`sandbox_profile_minimum` is a typed S3.2 `SandboxProfile`. It is the **minimum** sandbox the adapter commits to running under. It binds the L3 dispatch in two directions:

- **Composition floor.** The S3.2 composer (`ComposeProfile`) merges the manifest minimum with the policy's `Constraints.sandbox_profile_id`, the action subject's identity floor, the renderer surface's floor, and the runtime safety floor. The result is composed via the S3.2 §5.3 per-field rules — stricter wins. The merged profile is then compared against `sandbox_profile_minimum` at every field; if the merged profile is **less strict** at any field, the dispatch is refused with `SANDBOX_APPLICATION_FAILED` (and `ADAPTER_REGISTRATION_REJECTED` if the regression was caught at registration). The manifest minimum thus operates as an additional constitutional source in the §5.3 fall-through.
- **Cannot be loosened.** Per INV-017 (sandbox floor is constitutional), no source — policy bundle, app manifest, user request, adapter default — may compose below the runtime safety floor. The adapter manifest's `sandbox_profile_minimum` is at least as strict as the runtime safety floor for the relevant subject class (`human_initiated_floor` for human-only adapters; `ai_initiated_floor` for AI-reachable adapters; `recovery_mode_floor` for recovery adapters). Registration cross-checks this and rejects manifests whose minimum is weaker than the corresponding runtime safety floor.

The combined effect: the sandbox the adapter actually runs under is `max(runtime safety floor, manifest minimum, policy constraint, identity floor, renderer floor, app required)` — strictest wins everywhere. INV-017 binds.

#### §4.2.5 Declared failure modes

`declared_failure_modes` enumerates the failure modes the adapter may produce. The runtime uses this list for two purposes:

- **Health threshold sizing.** An adapter that declares `ADAPTER_TIMEOUT` as expected receives a timeout-rate budget proportional to the operational expectation; one that does not declare it gets a tighter threshold (a single timeout is more anomalous when not declared).
- **Failure-classification check.** When the adapter signals failure, the runtime checks the failure mode against the declared list. A failure mode outside the declared list is itself a `KIND_OR_CAPABILITY_OVERRUN` event (the adapter is exhibiting a behaviour outside its contract).

`KIND_OR_CAPABILITY_OVERRUN` MUST NOT appear in `declared_failure_modes` — the runtime treats it as a constitutional violation regardless of declaration. A manifest that lists it fails registration.

#### §4.2.6 Operational budgets

`default_adapter_timeout_seconds` is bounded by S10.1 §15 (queued for that section's expansion); the implementation enforces a hard ceiling of 600 s (10 minutes) for non-recovery adapters and 3600 s (1 h) for recovery-mode adapters. Per-action overrides via `AdapterActionDeclaration.timeout_seconds` are bounded the same way.

`default_rollback_timeout_seconds` follows the same bounding. Rollback timeout exhaustion is itself terminal (S10.1 §3.7 `RollbackOutcome.FAILED`).

#### §4.2.7 Network / external API enumeration

`network_outbound_hosts` is a closed list of FQDNs or CIDRs the adapter may dial. Composed against S8.1's `NetworkOutboundManifest` at sandbox composition. Free-form egress is impossible — an adapter that needs an unenumerated host fails the egress check at dispatch.

`external_api_hosts` is a closed list of FQDNs (HTTPS-only) the adapter may invoke. Cross-checked against `network_outbound_hosts` (subset relationship).

#### §4.2.8 Evidence-emit declared types

`declared_evidence_record_types` is non-empty only when `declared_capabilities` contains `EVIDENCE_EMIT`. The runtime cross-checks every adapter-emitted evidence record against this declared set; an emit outside the set is a `KIND_OR_CAPABILITY_OVERRUN` and forces `RETIRED`. Adapter-emitted evidence is rare; most adapters rely on the runtime to emit on their behalf. INV-015 (evidence never contains secrets) binds adapter-emitted records identically.

#### §4.2.9 Distribution provenance

`source_package_id` is the S11.1 package id the manifest was distributed under. `PackageKind = ADAPTER` (S11.1 §3.4). The package's `required_sandbox` (S11.1 §3) must be ≥ `sandbox_profile_minimum` — the runtime cross-checks at registration. A package that ships a weaker required sandbox than the manifest's minimum fails registration (`ADAPTER_REGISTRATION_REJECTED FOREVER`).

`publisher_root_id` is the S11.1 publisher catalog entry (`pubcat_<hex>`). The publisher must be at trust level `VERIFIED` or `AIOS_ROOT` for non-recovery adapters; recovery-mode adapters require `AIOS_ROOT`. `COMMUNITY` publishers may register adapters only with the `FILESYSTEM_READ` capability and only on user-installed scope (per S11.1 §3.4 — not in scope for full deepening here, see S11.1).

#### §4.2.10 Signature

`manifest_signature` is Ed25519 over `JCS(serialize(fields 1..20))` with the publisher's signing key. The signing key chain terminates at the AIOS root via the S11.1 publisher catalog (`AIOS root → publisher root → adapter signing key`, chain depth ≤ 3). `signing_key_id` identifies the signing key in the AIOS trust store.

`manifest_expires_at` is mandatory. An adapter whose manifest expires is automatically deregistered (§7.4). Operators rotate manifests by submitting `runtime.adapter.register` with the same `adapter_id`, a strictly greater `adapter_version`, and a fresh signature.

The signature covers fields 1..20, not the full record. Fields 21..24 (`manifest_signature`, `signing_key_id`, `manifest_created_at`, `manifest_expires_at`) are signature metadata; including them in the signed bytes would create a circular self-reference for `manifest_signature`. The two timestamp fields (23, 24) are co-attested by the publisher's signed envelope at S11.1's package level — if an attacker tries to extend `manifest_expires_at` post-sign, the package envelope's signature breaks (S11.1 §4 cross-package signing), and admission fails at §6.2.4 (publisher trust). The two-layer signing (manifest signature + S11.1 package envelope) is a defence-in-depth: the manifest is independently verifiable, and the package distribution path adds expiry/freshness binding.

#### §4.2.11 Field-set immutability

Once a manifest is `REGISTERED`, every field is immutable for the lifetime of that manifest snapshot. Any change requires a new manifest version (hot-reload, §7.1). The runtime keeps the manifest as a content-addressed AIOS-FS object; the address is the manifest digest (`BLAKE3(JCS(serialize(fields 1..20)))`), and the directory's per-`adapter_id` mapping points to the current snapshot's digest. In-flight actions reference the digest they were dispatched against; a hot-reload that registers a new digest does not retroactively rebind in-flight actions. This guarantees that the manifest a verification probe consults at `VerifyAction` (S2.4) is exactly the one the dispatch step honoured.

## §5 Registration FSM (closed)

The six states from §3.1 form a strictly closed FSM. Transitions not listed here are forbidden; an attempt to drive the FSM through an illegal transition is rejected and emits evidence (`ADAPTER_REGISTRATION_REJECTED` for admission-side illegal transitions; `ADAPTER_LIFECYCLE_ILLEGAL_TRANSITION` queued for runtime-side illegal transitions, mirrored to the evidence log).

### §5.1 Diagram

```text
                             +---------+
              submit         |  DRAFT  |
              ────────────►  +----+----+
                                  |
                                  | pre-validation passes
                                  v
                            +-----------+
                            |VALIDATING |
                            +-----+-----+
                              |       |
                              |       | reject (signature, trust, catalog,
                              |       |  capability-grant, schema)
                              |       v
                              |   ┌──────────────────┐
                              |   │  (rejected; no   │
                              |   │   directory      │
                              |   │   entry; FOREVER │
                              |   │   evidence)      │
                              |   └──────────────────┘
                              v
                         +-----------+
                         |REGISTERED |◄──────────┐
                         +-----+-----+           │
                          |    ^                 │ hot-reload
                          |    │ heal             │ (versioned update)
                          v    │                 │
                     +-----------+                │
                     | DEGRADED  |                │
                     +-----+-----+                │
                          |                       │
                          | escalate              │
                          v                       │
                    +--------------+              │
                    |DEREGISTERED  |──────────────┘
                    +--------------+
                          |
                          | kind-overrun OR capability-overrun OR
                          | signature forgery OR explicit retire
                          v
                    +-----------+
                    |  RETIRED  |  (terminal; constitutional ban)
                    +-----------+
```

### §5.2 Allowed transitions, exhaustive

| #    | From           | To             | Trigger                                                                                                                                                                                                            |
| ---- | -------------- | -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| AT1  | (init)         | `DRAFT`        | `runtime.adapter.register` typed action accepted; pre-validation pending.                                                                                                                                          |
| AT2  | `DRAFT`        | `VALIDATING`   | Pre-validation succeeded (schema parse, field-presence checks, identity-field consistency).                                                                                                                        |
| AT3  | `DRAFT`        | (rejected)     | Pre-validation failed (malformed proto, identity-field mismatch). Manifest discarded. `ADAPTER_REGISTRATION_REJECTED FOREVER` emitted.                                                                             |
| AT4  | `VALIDATING`   | `REGISTERED`   | All admission checks (§6.2) passed; manifest sealed in directory.                                                                                                                                                  |
| AT5  | `VALIDATING`   | (rejected)     | Any admission check failed (signature, trust chain, action-kind catalog cross-check, capability-grant cross-check, sandbox-floor regression). Manifest discarded. `ADAPTER_REGISTRATION_REJECTED FOREVER` emitted. |
| AT6  | `REGISTERED`   | `DEGRADED`     | Health signal crossed threshold (timeout rate, panic rate, preliminary kind/capability anomaly). `ADAPTER_DEGRADED EXTENDED_60M` emitted.                                                                          |
| AT7  | `DEGRADED`     | `REGISTERED`   | Health signal restored (rolling window clean for the threshold period). Auto-heal.                                                                                                                                 |
| AT8  | `DEGRADED`     | `DEREGISTERED` | Health signal escalated past auto-heal threshold or operator deregistered explicitly.                                                                                                                              |
| AT9  | `REGISTERED`   | `DEREGISTERED` | Voluntary withdrawal (`runtime.adapter.deregister`), manifest expiry, or operator deregistration without prior `DEGRADED`.                                                                                         |
| AT10 | `REGISTERED`   | `REGISTERED`   | Hot-reload (versioned manifest update). Old manifest snapshot remains active for in-flight actions; new dispatches use new manifest. (§7.) Self-loop.                                                              |
| AT11 | `DEREGISTERED` | `REGISTERED`   | Re-registration with same `adapter_id` and a strictly greater `adapter_version` (and full admission-check pass).                                                                                                   |
| AT12 | any state      | `RETIRED`      | Constitutional violation (kind-overrun, capability-overrun, signature forgery), OR explicit operator retirement (typed action `runtime.adapter.retire`). Terminal.                                                 |

Forbidden transitions (every attempt is itself a `ADAPTER_LIFECYCLE_ILLEGAL_TRANSITION` event and emits evidence):

- Any transition out of `RETIRED` — terminal, constitutional ban.
- `DRAFT → REGISTERED` directly (skipping `VALIDATING`) — admission checks are not optional.
- `VALIDATING → DEGRADED` — `DEGRADED` is reachable only from `REGISTERED`.
- `DEREGISTERED → REGISTERED` with the same or older `adapter_version` — re-admission requires monotonic version increase.
- `REGISTERED → REGISTERED` for an unsigned or stale-signature manifest — hot-reload requires fresh signature.

### §5.3 Persistence

Registration state persists in the runtime's authoritative AIOS-FS object (per S1.3 transactional semantics). Crash recovery preserves states: an adapter in `VALIDATING` at crash time resumes in `DRAFT` at boot (admission must be re-run from a known-clean point); an adapter in `REGISTERED` resumes in `REGISTERED`; `DEGRADED` resumes in `DEGRADED` with a fresh measurement window; `DEREGISTERED` and `RETIRED` are persistent. The conservative posture mirrors S10.1 §4.3: better an adapter observed as `DRAFT` and re-validated than an adapter observed as `REGISTERED` without a verified admission pass.

## §6 Registration discipline

### §6.1 Submission

Registration is itself a typed action: `runtime.adapter.register` (subject MUST be operator-class; AI subjects are hard-denied per INV-013). The action carries the candidate `AdapterManifest` in its `target`. The action flows through the standard S10.1 lifecycle — pre-validation, policy decision, approval (typically required), execution. The "execution" is the admission-check pipeline of §6.2; success transitions the adapter to `REGISTERED`.

### §6.2 Admission checks (strictly ordered)

The admission pipeline is six steps. Failure at any step terminates with `ADAPTER_REGISTRATION_REJECTED FOREVER`; no partial admission, no silent fallback, no "best-effort" registration.

#### §6.2.1 Schema parse and identity consistency

The manifest must parse against the `aios.adapter.v1alpha1.AdapterManifest` proto. The `adapter_id` token decomposition must match the explicit `vendor`, `name`, `adapter_version` fields verbatim. The `spec_version` must be a known schema version. Failure → `ADAPTER_REGISTRATION_REJECTED FOREVER`.

#### §6.2.2 Signature verification

`manifest_signature` is verified as Ed25519 over `JCS(serialize(fields 1..20))` with the public key identified by `signing_key_id`. The key chain is walked via the S11.1 publisher catalog; chain depth must be ≤ 3 (AIOS root → publisher root → signing key). Any verification failure or chain break → `MANIFEST_SIGNATURE_INVALID` runtime error and `ADAPTER_REGISTRATION_REJECTED FOREVER` evidence. The manifest is **not** retained in the directory; only the rejection evidence is.

#### §6.2.3 Manifest freshness

`manifest_expires_at` must be in the future at submission time. `manifest_created_at` must be within the past 90 days (replay defence — old manifests must be re-signed before re-admission). Failure → `ADAPTER_REGISTRATION_REJECTED FOREVER`.

#### §6.2.4 Publisher trust

The publisher identified by `publisher_root_id` is looked up in the S11.1 publisher catalog. The publisher's trust level must be ≥ the minimum required by the manifest's declared capabilities (per the §3.2 table — `VAULT_OPERATION` requires `VERIFIED`; `SCHEDULER_PRIVILEGED` requires `AIOS_ROOT`; etc.). `DEPRECATED` and `DEPLATFORMED` publishers are hard-denied at this step (no new admissions). Failure → `ADAPTER_REGISTRATION_REJECTED FOREVER`.

#### §6.2.5 Action-kind catalog cross-check + capability-grant cross-check

Each `declared_actions[i].action_kind` is looked up in the L5 capability catalog (S1.1 §6.4). Missing kinds → `ADAPTER_REGISTRATION_REJECTED FOREVER`. Action-kind exclusivity (S10.1 §10.5) is checked against the directory; a kind already owned by another `REGISTERED` adapter → reject.

Each `declared_capabilities[i]` is cross-checked against the registering subject's L4 grant capacity. The registering subject must hold sufficient grant capacity to **delegate** the capability to the adapter — a subject that cannot itself authorise `VAULT_OPERATION` cannot register an adapter that declares it. The check is: for every declared capability `c`, the registering subject's effective grants must include `adapter.delegate.<c>`. Missing grant → `ADAPTER_REGISTRATION_REJECTED FOREVER` with `reason = INSUFFICIENT_DELEGATION_GRANT`.

#### §6.2.6 Sandbox-floor consistency

`sandbox_profile_minimum` is compared against the runtime safety floor (S3.2 §5.4) for the relevant subject class. The minimum must be at least as strict as the floor at every field. Failure (manifest minimum weaker than floor at any field) → `ADAPTER_REGISTRATION_REJECTED FOREVER`.

The package's `required_sandbox` (S11.1 §3) is also cross-checked: the package's required sandbox must be at least as strict as the manifest's `sandbox_profile_minimum` (you cannot ship a weaker package than the adapter promises to run under). Failure → `ADAPTER_REGISTRATION_REJECTED FOREVER`.

#### §6.2.7 Sealing

On all six steps passing, the manifest is sealed in the adapter directory: stored as an immutable AIOS-FS object, indexed by `adapter_id`, registered in the action-kind ownership table, and announced to the runtime's directory consumers. The transition `VALIDATING → REGISTERED` (AT4) is recorded; `ADAPTER_REGISTERED STANDARD_24M` evidence is emitted.

### §6.2.8 Idempotency of admission

Re-submitting the same `(adapter_id, adapter_version, manifest_digest)` triple is idempotent: if the prior result was `REGISTERED`, the second submission is a no-op (`ADAPTER_REGISTRATION_REQUESTED` evidence emitted; no new directory mutation; the prior `REGISTERED` state stands). If the prior result was rejection, the second submission is rejected identically (and a fresh `ADAPTER_REGISTRATION_REJECTED FOREVER` is emitted — every rejection is recorded; the second is forensically distinct from the first because `idempotency_key` and submission timestamp differ at the action envelope level, even though the manifest payload is identical). The runtime does not coalesce repeat-rejection evidence; coalescing would lose the audit signal that an attacker is iterating against the admission boundary.

A submission with the same `(adapter_id, adapter_version)` but a **different** manifest digest is rejected with `ADAPTER_REGISTRATION_REJECTED FOREVER` and reason `MANIFEST_DIGEST_MISMATCH` — the same version cannot have two distinct manifest bodies. This closes a "silent re-spin" attack where a publisher tries to re-publish a version under a different signed body without bumping the version.

### §6.3 Capability honesty enforcement at dispatch

At every `ExecuteAction`, the runtime composes a `SandboxProfile` whose capability allow-list is the **intersection** of:

- `sandbox_profile_minimum` (the adapter manifest's floor),
- the policy decision's `Constraints.sandbox_profile_id` (S2.3 §10),
- the runtime safety floor (S3.2 §5.4 — strictest, constitutional),
- the action subject's identity floor.

A capability **outside** `declared_capabilities` is denied at the kernel boundary. An adapter that attempts a capability it did not declare hits a kernel-level denial (EPERM, EACCES, or the sandbox-mediated equivalent). The runtime observes the denial as a candidate `KIND_OR_CAPABILITY_OVERRUN` (§9 — `ADAPTER_CAPABILITY_VIOLATION FOREVER`) and forces the adapter to `RETIRED`.

This is INV-013 binding in two senses: AI cannot perform system admin (the AI subject is sandboxed below admin caps via `ai_initiated_floor`), and an adapter cannot lie about what it does (an undeclared capability is unreachable, not a privilege escalation). INV-017 binds: the floor is constitutional and the manifest's minimum is enforced as an additional source.

## §7 Hot-reload, expiry, deregistration

### §7.1 Versioned hot-reload

Operators rotate manifests by submitting a fresh `runtime.adapter.register` with the same `adapter_id`, a strictly greater `adapter_version`, and a fresh signature. The flow:

1. New manifest enters `DRAFT → VALIDATING` per §6.
2. Admission checks run identically. The action-kind catalog cross-check (§6.2.5) accepts kinds the **same** adapter already owns (it does not collide with itself); kinds not in the new manifest but present in the old are released back to the catalog.
3. On `VALIDATING → REGISTERED` of the new manifest, the runtime begins **action draining** on the old:
   - In-flight actions on the old manifest snapshot continue under the old manifest. The runtime preserves the old manifest as an immutable snapshot referenced by the in-flight actions' lifecycle records.
   - New `SubmitAction` against any of the adapter's declared kinds resolves to the **new** manifest.
4. When the last in-flight action on the old snapshot completes, the old snapshot is garbage-collected (retained only as evidence in the adapter directory's history).

`ADAPTER_HOT_RELOADED STANDARD_24M` evidence is emitted at step 3. The transition is the AT10 self-loop (`REGISTERED → REGISTERED`).

### §7.2 Manifest expiry

A `REGISTERED` adapter whose `manifest_expires_at` reaches the current TAI clock is automatically deregistered: `REGISTERED → DEREGISTERED` (AT9), `reason = MANIFEST_EXPIRED`, `ADAPTER_DEREGISTERED EXTENDED_60M` evidence emitted. In-flight actions complete on the existing snapshot per §7.1's draining rule. Operators must rotate before expiry to avoid service interruption.

### §7.3 Voluntary deregistration

The publisher (or operator on behalf of the publisher) submits `runtime.adapter.deregister` carrying the `adapter_id`. On admission of the typed action: `REGISTERED → DEREGISTERED` (AT9), `reason = VOLUNTARY`. In-flight actions drain per §7.1.

### §7.4 Operator-driven deregistration

An operator may deregister an adapter at any time (`runtime.adapter.deregister` with operator subject). Useful for security incident response (e.g. an unrelated CVE in the underlying tool the adapter wraps). Same transition; `reason = OPERATOR`.

### §7.5 Health-driven deregistration (DEGRADED → DEREGISTERED)

If `DEGRADED` persists past the auto-heal threshold (default: 24 hours of continuous degradation, or three consecutive measurement windows with degradation, whichever comes first), the runtime auto-deregisters: `DEGRADED → DEREGISTERED` (AT8), `reason = HEALTH_ESCALATION`. In-flight actions drain.

### §7.6 Constitutional retirement (any → RETIRED)

The constitutional break is `RETIRED` (AT12). Reached by:

- **Kind-overrun.** An adapter exhibits an action kind outside its declared set (S10.1 §12.5; §9 here). `ADAPTER_ACTION_KIND_VIOLATION FOREVER` emitted; immediate `RETIRED`.
- **Capability-overrun.** An adapter invokes a capability outside its declared set (caught at the sandbox boundary). `ADAPTER_CAPABILITY_VIOLATION FOREVER` emitted; immediate `RETIRED`.
- **Signature forgery (post-admission).** A manifest already in `REGISTERED` whose signing key is later revoked by the publisher (S11.1 publisher key rotation flow) is moved to `RETIRED` if the rotation evidence proves the prior key was compromised. `ADAPTER_REGISTRATION_REJECTED FOREVER` emitted retroactively (the in-flight admission record is voided).
- **Operator retirement.** A typed `runtime.adapter.retire` from an operator subject (typically a security-incident response action). `ADAPTER_DEREGISTERED EXTENDED_60M` emitted with `reason = OPERATOR_RETIRE`.

`RETIRED` is constitutionally terminal: the same `adapter_id` cannot be re-registered. A successor adapter must use a different `adapter_id` (different vendor or name). This forecloses "rebadge and re-admit" attacks where a malicious adapter is retired, then immediately re-admitted under the same id with a forged-clean manifest.

## §8 Health classification (REGISTERED ↔ DEGRADED)

### §8.1 Signals

The runtime maintains a per-adapter health window. Three signals feed it:

- **Timeout rate.** Number of `ADAPTER_TIMEOUT` events in the rolling 5-minute window. Threshold: 3 (S10.1 §12.8). Crossing → `DEGRADED`.
- **Panic rate.** Number of `ADAPTER_PANIC` events in the rolling 15-minute window. Threshold: 1. Single panic forces `DEGRADED` (S10.1 §12.9 already forces dispatch_kind = `SUBPROCESS_FORK` for 24 h; `DEGRADED` is the state correlate).
- **Preliminary overrun observation.** A single observation of an action-kind or capability anomaly that has not yet been confirmed as a constitutional violation (e.g. an adapter response carrying an unexpected field that _could_ be a kind-overrun once the validator confirms). Observation → `DEGRADED`; confirmation → `RETIRED`.

### §8.2 Auto-heal vs escalation

Auto-heal: a clean rolling window for 30 minutes (timeout-driven `DEGRADED`) or 60 minutes (panic-driven `DEGRADED`) returns the adapter to `REGISTERED` (AT7). `ADAPTER_HEALTHY` evidence emitted at the heal transition.

Escalation: degradation persisting past the auto-heal threshold (24 h continuous, or 3 consecutive measurement windows with degradation, whichever comes first) escalates to `DEREGISTERED` (AT8) with `reason = HEALTH_ESCALATION`.

### §8.3 Health vs stability — the orthogonality

`AdapterRegistrationState` (this sub-spec, §3.1) and `AdapterStability` (S10.1 §3.4) are orthogonal axes. Both apply simultaneously:

| Stability \ State | `REGISTERED`                                     | `DEGRADED`                                                | `DEREGISTERED` / `RETIRED`               |
| ----------------- | ------------------------------------------------ | --------------------------------------------------------- | ---------------------------------------- |
| `EXPERIMENTAL`    | AI-origin forced to `DRY_RUN` (S10.1 §3.4)       | AI-origin still forced; humans queued behind backpressure | New dispatches refused                   |
| `STABLE`          | Eligible for `IN_PROCESS_RPC` per S10.1 §3.2     | Falls back to `SUBPROCESS_FORK` regardless of preference  | New dispatches refused                   |
| `DEPRECATED`      | Emits `ADAPTER_DEPRECATED_DISPATCH` per dispatch | Same + degradation backpressure                           | New dispatches refused                   |
| `RETIRED` (S10.1) | (impossible — both are RETIRED-equivalent)       | (impossible)                                              | `ListAdapters` returns for forensic only |

The two axes do not collapse. The composer reads both. The S10.1 §3.4 `RETIRED` stability and this sub-spec's §3.1 `RETIRED` state are equivalent end-points that the runtime treats identically for dispatch (both refuse new actions); the distinction is which side of the contract caused the termination — stability is operator-driven (`runtime.adapter.set_stability`), state is admission/health/violation-driven.

## §9 Evidence record types

This sub-spec emits the closed list of ten evidence record types below. Each is **queued for addition** to the closed S3.1 `RecordType` enum (S3.1 §24.1; the additions advance the count by 10 once consolidated). Until S3.1 is amended, the runtime emits these record types as `RECORD_TYPE_UNSPECIFIED` with a typed payload extension; the consolidation pass will rewire them to the canonical enum values.

| Record type                      | Default retention | Emitted on                                                                                                                                                |
| -------------------------------- | ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ADAPTER_REGISTRATION_REQUESTED` | `STANDARD_24M`    | `runtime.adapter.register` accepted at validation; manifest enters `DRAFT`. Payload carries `adapter_id`, `adapter_version`, `publisher_root_id`.         |
| `ADAPTER_REGISTRATION_REJECTED`  | `FOREVER`         | Any admission check failed (§6.2.1 — §6.2.6). Payload carries the failed step id and the manifest digest. Constitutional forensic event.                  |
| `ADAPTER_REGISTERED`             | `STANDARD_24M`    | All admission checks passed; manifest sealed; `VALIDATING → REGISTERED` (AT4).                                                                            |
| `ADAPTER_HEALTHY`                | `STANDARD_24M`    | Adapter transitioned from `DEGRADED → REGISTERED` (AT7). Auto-heal observed.                                                                              |
| `ADAPTER_DEGRADED`               | `EXTENDED_60M`    | Adapter transitioned from `REGISTERED → DEGRADED` (AT6). Health threshold crossed.                                                                        |
| `ADAPTER_ACTION_KIND_VIOLATION`  | `FOREVER`         | Adapter served a response for an action kind outside its declared set. Forces `RETIRED`. Payload carries the offending kind.                              |
| `ADAPTER_CAPABILITY_VIOLATION`   | `FOREVER`         | Adapter invoked a capability outside its declared set (caught at sandbox boundary). Forces `RETIRED`. Payload carries observed capability + kernel error. |
| `ADAPTER_HOT_RELOADED`           | `STANDARD_24M`    | Versioned manifest update succeeded (§7.1). Old snapshot retained for in-flight; new snapshot active for new submissions.                                 |
| `ADAPTER_DOWNGRADE_REJECTED`     | `FOREVER`         | A registration attempt with `adapter_version` strictly less than the highest seen for `(vendor, name)`. Replay/downgrade defence.                         |
| `ADAPTER_DEREGISTERED`           | `EXTENDED_60M`    | Adapter removed from directory. Payload carries `reason ∈ {VOLUNTARY, MANIFEST_EXPIRED, OPERATOR, HEALTH_ESCALATION, OPERATOR_RETIRE}`.                   |

Each record's payload includes the relevant ids (`adapter_id`, `adapter_version`, `publisher_root_id`, `signing_key_id`, `manifest_digest`, the registering subject id, and the lifecycle state at emission). Secret-shaped redaction follows S3.1 §14 default profile. INV-015 (evidence never contains secrets) binds every payload — adapter manifests do not contain secrets, but the redactor is run unconditionally.

The runtime is the **only** authorised emitter for these ten record types. Append attempts from any other subject are hard-denied at the evidence log surface and themselves emit `TAMPER_DETECTED` per S3.1 §11.5.

## §10 Adversarial robustness

Closed threat model. Every defence below is constitutional; bundle authors cannot disable them.

### §10.1 Forged manifest signature

A manifest whose Ed25519 signature does not verify against any key in the publisher catalog trust chain is rejected at §6.2.2. `MANIFEST_SIGNATURE_INVALID` runtime error returned to the registering subject; `ADAPTER_REGISTRATION_REJECTED FOREVER` evidence emitted; manifest discarded. The adapter does not enter the directory. A subsequent `SubmitAction` for the would-be-served kinds fails with `UNKNOWN_ACTION_KIND`. This is the constitutional admission boundary — no "best-guess admit" path exists. INV-002 and INV-014 bind.

### §10.2 Runtime kind-overrun (post-admission)

An adapter that was admitted with `declared_actions = [A, B]` and emits a response for kind `C` is detected at response validation (S10.1 §12.5 partial; this section is the deepening). The runtime:

1. Rejects the response. The action transitions to `FAILED` with `ExecutionFailureReason = ADAPTER_REFUSED` (the closest S10.1 reason; the deeper `KIND_OR_CAPABILITY_OVERRUN` is the §3.5 manifest-side classification).
2. Forces the adapter to `RETIRED` (any → `RETIRED`, AT12). Constitutional ban — same `adapter_id` cannot re-register.
3. Emits `ADAPTER_ACTION_KIND_VIOLATION FOREVER` with the offending kind, the response digest, the manifest digest at admission.
4. Cancels in-flight actions on the same adapter (they transition to `FAILED` with `ADAPTER_PANIC` reason, since the adapter cannot be trusted mid-flight).
5. Releases the action-kind ownership table entries.

The escalation is one-shot. There is no "warn first, retire on second offence" — kind-overrun is a manifest lie and the constitutional posture is zero-tolerance.

### §10.3 Capability-overrun (post-admission)

An adapter whose `declared_capabilities` does not include `NETWORK_OUTBOUND` that attempts an outbound socket() syscall hits the seccomp boundary of the composed sandbox (per §6.3 — the sandbox composition uses `declared_capabilities` to size the syscall allow-list). The kernel returns EPERM. The runtime observes a sandbox denial event (S3.2 backend probe), classifies it as a candidate capability-overrun, and:

1. Confirms the classification — the denied syscall corresponds to a capability the manifest did not declare.
2. Forces `RETIRED` (AT12).
3. Emits `ADAPTER_CAPABILITY_VIOLATION FOREVER` with the observed capability class, the kernel error, the manifest digest.
4. Cancels in-flight actions.
5. Releases ownership.

Same one-shot posture as §10.2.

### §10.4 Manifest replay (downgrade attack)

An attacker, having obtained a previously-signed but later-superseded manifest, attempts to register it under the same `adapter_id`. The runtime tracks the highest `adapter_version` ever registered (or rejected) for `(vendor, name)`. The replayed manifest's `adapter_version` is strictly less than the high-water mark.

Defence: §6.2 step §6.2.1 includes a version-monotonicity check against the directory's high-water mark for `(vendor, name)`. A strictly-older version is rejected with `ADAPTER_DOWNGRADE_REJECTED FOREVER` evidence. The replay is recorded but does not enter the directory. The high-water mark is itself stored in the directory's append-only history; an attacker cannot rewrite it because INV-014 (no proof, no completion) and the evidence log invariants bind.

Combined with the manifest freshness check (§6.2.3 — `manifest_created_at` must be ≤ 90 days old), the replay window is bounded both by version monotonicity and by signing recency.

### §10.5 Forged trust chain

An attacker submits a manifest signed by a key whose chain claims to terminate at the AIOS root but does not. The S11.1 publisher catalog is the authoritative trust mapping; chain walking dereferences only catalog-listed entries. A claimed chain that includes an unrecognised intermediate or a revoked entry fails §6.2.2 (signature) or §6.2.4 (publisher trust). `ADAPTER_REGISTRATION_REJECTED FOREVER`.

### §10.6 Capability lie at registration

An adapter declares `declared_capabilities = [FILESYSTEM_READ]` but its package binary actually performs `FILESYSTEM_WRITE`. The lie is not detectable at registration (the runtime cannot statically prove syscall behaviour). However, at dispatch time (§6.3), the composed sandbox's capability allow-list is the intersection that excludes `FILESYSTEM_WRITE`. The first write attempt hits a sandbox denial → §10.3 capability-overrun → `RETIRED`. The lie is detected at first execution, not at admission. The constitutional posture: declarations bound execution; lies are absorbed by the sandbox floor; FOREVER evidence preserves the forensic trail.

### §10.7 Insufficient delegation grant

An attacker subject (operator-class but with reduced grant capacity) submits a registration for an adapter declaring `VAULT_OPERATION`. The §6.2.5 capability-grant cross-check rejects with `INSUFFICIENT_DELEGATION_GRANT`. `ADAPTER_REGISTRATION_REJECTED FOREVER`. The attacker cannot escalate by submitting an adapter that would itself perform what the attacker cannot.

### §10.8 Sandbox-floor regression at hot-reload

A hot-reload manifest declares `sandbox_profile_minimum` weaker (at any field) than the previously-registered version's minimum. §6.2.6 detects the regression and rejects (`ADAPTER_REGISTRATION_REJECTED FOREVER`). The previous manifest stays active. This prevents a "stealth weakening" attack where an attacker who has compromised the publisher's signing key tries to slowly relax the adapter's floor across rotations.

### §10.9 Manifest-bypass attempt at dispatch

A caller invokes `ExecuteAction` referencing an action kind whose adapter is `RETIRED` or `DEREGISTERED`. Per S10.1 §12.11 and §3.4, the runtime returns `UNKNOWN_ACTION_KIND` or `ADAPTER_NOT_DISPATCHABLE`. There is no fallback to a "default adapter". Unsupported actions fail closed (`00_overview.md` invariant).

### §10.10 Subject act-as forgery

The S0.1 §10.6 subject-cert binding is enforced at the public ingress; this sub-spec inherits the rule. A registering peer must match the registering subject's mTLS client cert (or be authorised to act-as via L4 policy). Mismatch → ingress-level `PERMISSION_DENIED`; no L3 admission lifecycle is created; no evidence in this sub-spec's vocabulary is emitted (the ingress emits its own `IDENTITY_BIND_FAILED`).

### §10.11 Concurrent registration races

Two operator subjects submit `runtime.adapter.register` for two manifests that both claim the same `action_kind`. Both reach §6.2.5 within the action-kind ownership table. The directory's append serialises them: the first to acquire the table's per-kind lock wins admission; the second receives `ADAPTER_KIND_COLLISION` (S10.1 §10.5) and `ADAPTER_REGISTRATION_REJECTED FOREVER`. Determinism is provided by the underlying AIOS-FS WAL ordering (S2 §03 transactional semantics) — there is no "tie" outcome and no double-admit. Both registrations leave evidence; only one mutates the directory.

### §10.12 Manifest-aliasing attempt

A manifest declares `vendor = "aiosroot"` while signed by a non-`AIOS_ROOT` publisher (an attempt to typosquat a privileged vendor). §6.2.4 cross-checks the vendor token in `adapter_id` against the publisher catalog: vendor strings under reserved namespaces (`aiosroot`, `aios`, `recovery`) are pinned to specific publisher root ids in the catalog. Mismatch → `ADAPTER_REGISTRATION_REJECTED FOREVER` with reason `RESERVED_VENDOR_NAMESPACE`. The publisher catalog is the only authority for namespace ownership.

## §11 Performance contract

### §11.1 Per-dispatch-kind registration check budgets

The "registration check" is the time taken by the runtime to confirm an admitted adapter is actually dispatchable for a single action — i.e. the per-dispatch overhead the adapter directory contributes to `ExecuteAction`. It is distinct from the full admission pipeline (§6.2), which is a one-time cost amortised across all later dispatches.

| `AdapterDispatchKind` | Registration-check p95 | Notes                                                                                                                         |
| --------------------- | ---------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `IN_PROCESS_RPC`      | < 1 ms                 | In-memory directory lookup, capability-set intersection, sandbox-profile-id resolution. No syscalls.                          |
| `SUBPROCESS_FORK`     | < 5 ms                 | Plus per-action subprocess spawn cost (not counted here; counted under S10.1 §14.3 dispatch overhead).                        |
| `ISOLATED_SANDBOX`    | < 50 ms                | Plus sandbox composition (S3.2 §6.1 perf budget) and apply (S3.2 backend probe). ~50 ms covers directory + composer hot path. |
| `DRY_RUN`             | < 1 ms                 | Same as `IN_PROCESS_RPC` for the directory side; the adapter side runs in simulation.                                         |

### §11.2 Admission pipeline budgets

The full admission pipeline (§6.2) is a typed-action workflow; it inherits S10.1's `ExecuteAction` budget for orchestration overhead (< 25 ms) plus the per-step costs:

| Step                               | p95     | Hard timeout |
| ---------------------------------- | ------- | ------------ |
| §6.2.1 Schema parse & identity     | < 1 ms  | 100 ms       |
| §6.2.2 Signature verification      | < 5 ms  | 200 ms       |
| §6.2.3 Manifest freshness          | < 1 ms  | 50 ms        |
| §6.2.4 Publisher trust             | < 5 ms  | 200 ms       |
| §6.2.5 Catalog + grant cross-check | < 10 ms | 500 ms       |
| §6.2.6 Sandbox-floor consistency   | < 5 ms  | 200 ms       |
| §6.2.7 Sealing                     | < 10 ms | 500 ms       |

Total p95 admission: < 50 ms. Hard total timeout: 5 s (admission is itself an `ExecuteAction` with a generous timeout because typed-action workflow overhead dominates for slow networks).

### §11.3 Directory size bounds

The runtime supports up to 1 024 simultaneously `REGISTERED` adapters. This is a defaulted operator-tunable bound; AIOS does not anticipate productions exceeding it. Adapters in `DEREGISTERED` and `RETIRED` are not counted against the live bound but are counted against the directory's history (for forensic queries); the historical bound is 65 536 entries, after which the oldest `DEREGISTERED` entries are summarised to a digest in S3.1 (`RETIRED` entries are never summarised — they remain at FOREVER fidelity per §9).

### §11.4 Observability surface

The adapter directory exposes the following gauges and counters to the L9.1 observability pipeline (sized per S10.1 §14 conventions):

| Metric                                         | Type    | Labels                                             | Notes                                                            |
| ---------------------------------------------- | ------- | -------------------------------------------------- | ---------------------------------------------------------------- |
| `aios_adapter_registered_total`                | gauge   | `state`                                            | Count of adapters per `AdapterRegistrationState`.                |
| `aios_adapter_admission_duration_seconds`      | histo   | `result ∈ {accepted, rejected}`, `failed_step`     | Per-admission latency; matches §11.2 budgets.                    |
| `aios_adapter_dispatch_check_duration_seconds` | histo   | `dispatch_kind`                                    | Per-dispatch directory-side cost; matches §11.1.                 |
| `aios_adapter_health_signal_total`             | counter | `adapter_id`, `signal ∈ {timeout, panic, overrun}` | Health-window inputs.                                            |
| `aios_adapter_violations_total`                | counter | `adapter_id`, `kind ∈ {action_kind, capability}`   | Constitutional violations; correlates with §9 evidence.          |
| `aios_adapter_rejections_total`                | counter | `failed_step`, `reason`                            | Admission rejections; correlates with §9 evidence.               |
| `aios_adapter_history_entries`                 | gauge   |                                                    | Total directory history entries (live + DEREGISTERED + RETIRED). |

These metrics are observation-only; they do not feed back into the constitutional path. The constitutional decisions (admit / reject / retire) are evidence-driven via §9, not metric-driven.

## §12 Worked examples

Three end-to-end examples. Each walks the registration FSM, the dispatch interaction, and at least one fail-closed path.

### §12.1 Filesystem adapter (`adapter:aiosroot:fs-aios:1.4.2`)

**Use case.** AIOS-FS write adapter for human-initiated and AI-proposed filesystem mutations under `/aios`. Used for nearly every typed action that creates or updates an AIOS-FS object.

**Manifest highlights.**

```yaml
adapter_id: "adapter:aiosroot:fs-aios:1.4.2"
declared_actions:
  - action_kind: "fs.aios.create_object"
  - action_kind: "fs.aios.update_object"
  - action_kind: "fs.aios.delete_object"
  - action_kind: "fs.aios.snapshot_create"
declared_capabilities:
  - FILESYSTEM_WRITE
  - FILESYSTEM_READ
  - EVIDENCE_EMIT
declared_invariants_supported:
  ["INV-002", "INV-008", "INV-013", "INV-014", "INV-015", "INV-017"]
io_mode: TYPED_PARAMETERS_ONLY
preferred_dispatch_kind: SUBPROCESS_FORK
declared_stability: STABLE
sandbox_profile_minimum:
  filesystem_mounts: ["/aios:rw", "/var/lib/aios:rw"]
  network: "none"
  cpu_weight: 100
  syscall_allow: ["read", "write", "openat", "fsync", "..."] # tightened
declared_failure_modes:
  [
    ADAPTER_TIMEOUT,
    ADAPTER_PANIC,
    RESOURCE_BUDGET_EXCEEDED,
    BACKEND_UNAVAILABLE,
    ROLLBACK_PRECONDITION_FAILED,
  ]
default_adapter_timeout_seconds: 30
declared_evidence_record_types: ["AIOS_OBJECT_WRITE_OBSERVED"]
source_package_id: "pkg:aiosroot:adapter-fs-aios:1.4.2"
publisher_root_id: "pubcat_aiosroot_a1b2..."
manifest_signature: <Ed25519>
signing_key_id: "key_aiosroot_2026q2"
```

**Registration walk.** Submitted by the AIOS root operator. §6.2.1 — schema OK. §6.2.2 — signature verifies against AIOS root. §6.2.3 — freshness OK. §6.2.4 — publisher = AIOS root, trust = `AIOS_ROOT`, sufficient for all declared capabilities. §6.2.5 — all four action kinds exist in the L5 capability catalog; none collide with another `REGISTERED` adapter; AIOS root holds delegation capacity for all declared capabilities. §6.2.6 — `sandbox_profile_minimum` is at least as strict as `human_initiated_floor` (and `ai_initiated_floor` for AI-reachable kinds). Sealed; `ADAPTER_REGISTERED` emitted.

**Happy-path dispatch.** A typed action `fs.aios.create_object` for a human-origin operator. S10.1 §3.2 decision rule: subject not AI, risk not privileged, manifest preferred `SUBPROCESS_FORK` → dispatch as `SUBPROCESS_FORK`. Composed sandbox: intersect manifest minimum, policy constraint, runtime safety floor — all aligned with manifest minimum. Adapter writes the object; verification probe confirms object hash. `SUCCEEDED`.

**Fail-closed path (capability lie).** A latent bug in adapter version 1.4.2 attempts to open a TCP socket (no `NETWORK_OUTBOUND` capability declared). The composed sandbox's seccomp blocks `socket()`. Adapter receives EPERM. Runtime observes the sandbox denial → §10.3 capability-overrun → `RETIRED`. `ADAPTER_CAPABILITY_VIOLATION FOREVER` emitted. The successor adapter must be `adapter:aiosroot:fs-aios2:*` (different name) — version 1.4.2's `adapter_id` is constitutionally banned.

### §12.2 Networking adapter (`adapter:netvendor:dns-aios:0.9.1-experimental`)

**Use case.** DNS plan/apply adapter for ProxGuard-style network configuration. New adapter, not yet hardened — operator promoted it to `EXPERIMENTAL` after admission.

**Manifest highlights.**

```yaml
adapter_id: "adapter:netvendor:dns-aios:0.9.1-experimental"
declared_actions:
  - action_kind: "net.dns.plan"
  - action_kind: "net.dns.apply"
declared_capabilities:
  - FILESYSTEM_READ          # read /etc/resolv.conf, AIOS-FS DNS objects
  - FILESYSTEM_WRITE         # write AIOS-FS DNS objects
  - NETWORK_OUTBOUND         # query upstream DNS for plan validation
  - SERVICE_LIFECYCLE        # reload systemd-resolved
declared_invariants_supported: ["INV-002","INV-008","INV-013","INV-014","INV-017"]
io_mode: TEMPLATE_PARAMETERS
preferred_dispatch_kind: ISOLATED_SANDBOX
declared_stability: EXPERIMENTAL
sandbox_profile_minimum: { filesystem_mounts: ["/aios/network:rw"], network: { allow: ["1.1.1.1","8.8.8.8"] }, ... }
declared_failure_modes: [ADAPTER_TIMEOUT, ADAPTER_PANIC, BACKEND_UNAVAILABLE, ADAPTER_REFUSED]
network_outbound_hosts: ["1.1.1.1","8.8.8.8"]
default_adapter_timeout_seconds: 60
source_package_id: "pkg:netvendor:adapter-dns-aios:0.9.1"
publisher_root_id: "pubcat_netvendor_..."
publisher trust = VERIFIED
```

**Registration walk.** §6.2.1 — schema OK. §6.2.2 — signature verifies (publisher signing key chain valid). §6.2.3 — freshness OK. §6.2.4 — publisher trust = `VERIFIED`, sufficient for all declared capabilities. §6.2.5 — kinds exist in the catalog; not colliding; operator subject holds delegation capacity. §6.2.6 — minimum sandbox stricter than `ai_initiated_floor` for network egress. Sealed; `ADAPTER_REGISTERED` emitted.

**Dispatch (AI-origin proposes `net.dns.apply`).** Subject is AI. S10.1 §3.2 forces `ISOLATED_SANDBOX` regardless of manifest preference (here the preference is already `ISOLATED_SANDBOX`). `EXPERIMENTAL` stability + AI subject → policy default forces `DRY_RUN` unless explicit policy clearance (S10.1 §3.4); without clearance the dispatch becomes `DRY_RUN`. Adapter produces a simulation transcript. Verification (read-only DNS probes) confirms the simulation is consistent. `SUCCEEDED` with `EXPERIMENTAL_ADAPTER_LIVE_DISPATCH` not emitted (because dispatch was simulated, not live).

**Fail-closed path (downgrade attack).** Attacker (having compromised the publisher's old signing key, which has since been rotated) submits version `0.9.0-experimental` (older than the current high-water mark of `0.9.1-experimental`). §6.2.1 detects the version-monotonicity violation. `ADAPTER_DOWNGRADE_REJECTED FOREVER` emitted. The old key's compromise is separately handled via the S11.1 publisher key rotation flow; once the rotation evidence is in, the old `0.9.1-experimental` itself is retroactively retired (§7.6 — signature forgery clause).

### §12.3 GPU adapter (`adapter:aiosroot:gpu-inference:2.0.0`)

**Use case.** Local LLM inference adapter, GPU-bound. Used by the L5 cognitive core for on-device model serving.

**Manifest highlights.**

```yaml
adapter_id: "adapter:aiosroot:gpu-inference:2.0.0"
declared_actions:
  - action_kind: "ai.inference.run"
  - action_kind: "ai.inference.warm_model"
declared_capabilities:
  - GPU_COMPUTE
  - FILESYSTEM_READ        # model weights, prompts
  - VAULT_OPERATION        # KEY_UNWRAP for encrypted model weights
declared_invariants_supported: ["INV-002","INV-008","INV-014","INV-015","INV-017","INV-024"]
io_mode: TYPED_PARAMETERS_ONLY
preferred_dispatch_kind: SUBPROCESS_FORK
declared_stability: STABLE
sandbox_profile_minimum:
  gpu_compute: { allow: ["nvidia0"], compute_heavy: true, memory_bytes_max: 8589934592 }
  filesystem_mounts: ["/aios/models:ro"]
  vault: { allow_operations: ["KEY_UNWRAP"] }
  ...
declared_failure_modes: [ADAPTER_TIMEOUT, ADAPTER_PANIC, RESOURCE_BUDGET_EXCEEDED, BACKEND_UNAVAILABLE, ADAPTER_REFUSED]
default_adapter_timeout_seconds: 300
publisher_root_id: "pubcat_aiosroot_..."
publisher trust = AIOS_ROOT
```

**Registration walk.** §6.2.1 — schema OK. §6.2.2 — AIOS root signature verifies. §6.2.3 — freshness OK. §6.2.4 — `AIOS_ROOT` trust, sufficient for all capabilities including `GPU_COMPUTE`. §6.2.5 — kinds in catalog; capabilities cross-check against L4 grant capacity (AIOS root holds the `gpu.compute_heavy` grant per L0 INV-024 — this is the constitutional gate; without that grant the registration would fail even for AIOS root, because INV-024 binds). §6.2.6 — minimum sandbox includes the `gpu.compute_heavy` capability allow-list and the GPU memory cap; stricter than `ai_initiated_floor` for compute GPU. Sealed; `ADAPTER_REGISTERED` emitted.

**Dispatch (AI subject proposes `ai.inference.run` for the cognitive core's own use).** Subject is AI. `ISOLATED_SANDBOX` forced. Composed sandbox: GPU compute access constrained to `nvidia0`, 8 GB cap, vault unwrap permitted only for the wrapped-model-key's vault id. Adapter unwraps the model key via Vault Broker (raw key never crosses the adapter boundary; see L4.2), runs inference, returns the response. Verification confirms the response shape. `SUCCEEDED`.

**Fail-closed path (capability escalation attempt).** Adapter version 2.0.0 has a regression that attempts to unwrap a vault id outside its grant. Vault Broker rejects with the canonical `VAULT_OP_DENIED`. Adapter receives the error; runtime observes the failure and classifies it. The adapter did not exceed its **declared** capability set (`VAULT_OPERATION` is declared); it exceeded its **grant capacity** for a specific vault id. This is a runtime-side (S10.1) `ADAPTER_REFUSED` failure, not a manifest-level capability-overrun. The action transitions to `FAILED`; the adapter remains `REGISTERED` (no kind/capability lie). The cognitive core re-proposes against a smaller scope.

**Fail-closed path (capability lie at hot-reload).** A 2.0.1 manifest is submitted that drops `gpu_compute.compute_heavy` from `sandbox_profile_minimum` (in an attempt to broaden GPU compute eligibility for non-`gpu.compute_heavy`-granted subjects). §6.2.6 detects the regression vs the prior 2.0.0 minimum. §10.8 — sandbox-floor regression at hot-reload — rejects with `ADAPTER_REGISTRATION_REJECTED FOREVER`. The 2.0.0 manifest stays active.

## §13 Cross-spec contract

This sub-spec is consumed by:

- **S10.1 (`03_capability_runtime_grpc.md`).** The S10.1 manifest skeleton's contract is operationalised here. The S10.1 `runtime.adapter.register` typed action's "execution" is the §6.2 admission pipeline. The S10.1 `ListAdapters` and `GetAdapterCapabilities` RPCs read from the directory this sub-spec defines. The S10.1 `ExecutionFailureReason` enum is referenced (and partially mirrored) by `AdapterFailureMode`.
- **S2.3 Policy Kernel (`L4_Policy_Identity_Vault/01_policy_kernel.md`).** Policy decisions read `declared_capabilities` to evaluate capability requirements; policy bundles cannot compose below the manifest's `sandbox_profile_minimum`.
- **S3.2 Sandbox Composition (`L6_Apps_Packages_Compatibility/04_sandbox_composition.md`).** The `sandbox_profile_minimum` is an additional source in the §5.3 fall-through; the `ComposeProfile` algorithm cannot drop below it.
- **S11.1 Repository Model (`L10_Distribution_Ecosystem_Marketplace/01_repository_model.md`).** Adapters are distributed as `PackageKind = ADAPTER`; the package's `required_sandbox` ≥ manifest `sandbox_profile_minimum` constraint is cross-checked at admission.
- **S2.4 Verification Engine.** The adapter's per-action `response_schema` feeds the verification grammar's response-shape check.
- **L0 invariants.** INV-002 (AI proposes never executes), INV-008 (default-deny), INV-013 (AI cannot system admin), INV-014 (no proof no completion), INV-015 (evidence never contains secrets), INV-017 (sandbox floor constitutional). All bind here.

## §14 Status, evidence grade, gates

| Aspect                                 | Status | Evidence grade | Notes                                                                                                       |
| -------------------------------------- | ------ | -------------- | ----------------------------------------------------------------------------------------------------------- |
| `AdapterManifest` schema               | `REAL` | E1             | File exists; structural contract complete; proto IDL inline.                                                |
| `AdapterRegistrationState` FSM         | `REAL` | E1             | Six states; allowed/forbidden transitions enumerated.                                                       |
| `AdapterCapabilityClass` (10)          | `REAL` | E1             | Closed enum; trust-level minima specified; honesty enforced at three points (§6.3).                         |
| `AdapterIOMode`, `AdapterDispatchKind` | `REAL` | E1             | Mirrors of S10.1.                                                                                           |
| `AdapterFailureMode` (10)              | `REAL` | E1             | Closed; mirrors and extends S10.1's `ExecutionFailureReason`.                                               |
| Registration discipline (§6.2)         | `REAL` | E1             | Six steps; strict ordering; fail-closed at every step.                                                      |
| Hot-reload + draining (§7.1)           | `REAL` | E1             | Versioned; in-flight actions drain on old snapshot.                                                         |
| Adversarial robustness (§10)           | `REAL` | E1             | Ten threat scenarios; each closed defence cited.                                                            |
| Performance contract (§11)             | `REAL` | E1             | Per-dispatch-kind budgets; admission pipeline budgets.                                                      |
| Ten evidence record types (§9)         | `REAL` | E1             | Queued for S3.1 enum addition; emitted as `RECORD_TYPE_UNSPECIFIED` with typed payload until consolidation. |
| Three worked examples (§12)            | `REAL` | E1             | Filesystem, networking, GPU adapters; happy-path + fail-closed for each.                                    |

E2 evidence (typecheck-clean proto IDL) is reached when the schema package `aios.adapter.v1alpha1` is extracted from this sub-spec into a `.proto` file and compiled with the rest of the AIOS schema package. E3 evidence requires unit and integration tests against the registration FSM and the §6.2 admission pipeline under all closed failure paths (§10). E4 requires end-to-end execution of the §12 worked examples against a working Capability Runtime instance with real adapters, with all evidence reconstructible from L9.1. Full E5 (live operational) is reached only after the runtime is deployed and has produced evidence in non-simulation mode against multiple production adapters across all three example classes.

## §15 See also

- [S10.1 — Capability Runtime gRPC](./03_capability_runtime_grpc.md)
- [S0.1 — Action Envelope + Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S3.2 — Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [S11.1 — Repository Model](../L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [L0 Invariants](../L0_Governance_Evidence_Safety/04_invariants.md) (INV-002, INV-008, INV-013, INV-014, INV-015, INV-017)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
