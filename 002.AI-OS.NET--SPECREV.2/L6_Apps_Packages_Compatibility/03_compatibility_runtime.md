# Compatibility Runtime — Orchestration of EcosystemRuntime Adapters (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists, structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Phase tag      | S12.3                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Layer          | L6 Apps, Packages, Compatibility                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| Schema package | `aios.compatruntime.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Consumes       | L0 INV-002 (AI proposes never executes), INV-008 (default-deny), INV-011 (cross-group access forbidden), INV-013 (AI cannot perform system admin), INV-017 (sandbox floor constitutional); S12.1 App Runtime Model (`EcosystemRuntime`, `EcosystemHonestyClass`, Phase A/B/C/D mechanism); S3.2 Sandbox Composition (`SandboxProfile`, `CompatibilityKind`, 5+1 source merge, runtime safety floor, §20 `ecosystem_runtime` field consolidation); S11.1 Repository Model (signed package manifest with `ecosystem_runtime` field, `PackageKind = ADAPTER`); S8.2 GPU Resource Model (per-group `VkDevice`, `GpuPolicy`); S0.1 Action Envelope (typed action `app.launch`, lifecycle FSM); S2.4 Verification Grammar (post-launch verification probe); S3.1 Evidence Log (`RecordType`) |
| Produces       | typed `OrchestrationKind` / `LaunchOutcome` / `WinePrefixKind` / `WaydroidIsolationLevel` / `VMFallbackKind` enums; the `app.launch` orchestration flow that composes a sandbox via S3.2 and invokes the EcosystemRuntime adapter named in the composed profile; per-orchestration-kind launch budgets; ten evidence record types queued for S3.1 Wave 8 consolidation                                                                                                                                                                                                                                                                                                                                                                                                                 |

## 1. Purpose

S12.1 closed the question "what foreign-code ecosystems can AIOS run, and how do we honestly disclose what each one can and cannot do?" S3.2 closed the question "given a typed action and the five composition sources, what sandbox profile binds the process?" S11.1 closed the question "what does a signed app package look like, and how does it land on the host?"

What no contract has yet closed is the question **between** those three: when the operator presses "launch" on an installed AIOS app, what exactly happens? Which EcosystemRuntime adapter is called? In what order are the sandbox composition, the runtime adapter invocation, and the verification probe run? Where does the launch fail closed if any layer disagrees? What is the timing budget for a launch on a cold Wine prefix vs. a warm Waydroid container vs. a freshly booted KVM guest?

This sub-spec is the **Compatibility Runtime orchestration contract**. It does not redefine `EcosystemRuntime`; that enum lives in S12.1 and is the canonical source. It does not redefine `SandboxProfile`; that lives in S3.2. It does not redefine the `app.launch` typed action shape; that envelope is owned by S0.1 and the per-action target schema is owned by the L6 app adapter manifest. What this contract adds is the **orchestration layer above S3.2 and below S10.1's adapter dispatch** that:

1. takes a typed `app.launch` action whose target carries an installed `app_id`;
2. resolves that app's manifest (§5) and pulls the declared `EcosystemRuntime` plus the requested orchestration semantics;
3. calls S3.2 `ComposeProfile` with the EcosystemRuntime baked into the `AdapterDefault` source so the composition's `ecosystem_runtime` field (S3.2 §20) is pinned;
4. selects exactly one `OrchestrationKind` from the closed eight-value enum in §3.1 — every selection is recorded in evidence;
5. invokes the EcosystemRuntime adapter (the Proton adapter, the Waydroid adapter, the KVM adapter, etc., each itself a `PackageKind = ADAPTER` per S11.1 §3.4) with the composed and applied sandbox profile;
6. starts the app process inside the sandbox boundary the composer just installed;
7. waits for the app's first health signal via an S2.4 verification probe (§7) within the per-`OrchestrationKind` budget;
8. emits the launch outcome from the closed seven-value `LaunchOutcome` enum and queues the appropriate evidence records.

The contract treats every launch as a typed AIOS action subject to the same sandbox-floor and policy discipline as every other action. There is no "shell out to wine64", no "just exec waydroid", no untyped subprocess fork that escapes the S3.2 floor. The cost of that uniformity is bounded — the launch budgets in §6 define honest worst-case latency for each orchestration kind so the operator and the L7 marketplace surface can show realistic expectations rather than pretending every Windows game launches in 200 ms.

## 2. Position in the system

```text
              ┌────────────────────────────────────────────────────────────────┐
              │                          OPERATOR                              │
              │                  (or AI proposing app.launch                   │
              │                  through the Cognitive Core)                   │
              └────────────────────────────────────────────────────────────────┘
                                          │
                                          │ typed action: app.launch
                                          │   target = { app_id: "app:..." }
                                          ▼
              ┌────────────────────────────────────────────────────────────────┐
              │  S0.1 ENVELOPE INGRESS  (CREATED → POLICY_PENDING)             │
              │  S10.1 ValidateAction → EvaluatePolicy →                       │
              │  RequestApproval (if required) → enter EXECUTING               │
              └────────────────────────────────────────────────────────────────┘
                                          │
                                          ▼
              ┌────────────────────────────────────────────────────────────────┐
              │  S12.3 COMPATIBILITY RUNTIME ORCHESTRATOR  (this contract)     │
              │                                                                │
              │   step A: resolve installed app manifest at /aios/.../apps/    │
              │           pull declared EcosystemRuntime (S12.1)               │
              │   step B: build CompositionInputs with EcosystemRuntime baked  │
              │           into AdapterDefault.ecosystem_runtime                │
              │   step C: call S3.2 ComposeProfile → SandboxProfile            │
              │   step D: select OrchestrationKind (this contract §3.1)        │
              │   step E: call S3.2 ApplyProfile → enforcement loaded          │
              │   step F: invoke EcosystemRuntime adapter under the applied    │
              │           profile; start app process inside sandbox            │
              │   step G: post-launch verification probe (S2.4)                │
              │   step H: emit LaunchOutcome + evidence (this contract §11)    │
              └────────────────────────────────────────────────────────────────┘
                                          │
              ┌───────────────────────────┼───────────────────────────────────┐
              │                           │                                    │
              ▼                           ▼                                    ▼
       ┌────────────┐            ┌────────────────┐                ┌─────────────────────┐
       │ LAUNCHED   │            │ LAUNCH_FAILED_*│                │ TIMED_OUT (budget   │
       │ (steady    │            │ (closed enum   │                │ exceeded;           │
       │  state;    │            │  §3.2)         │                │ partial cleanup)    │
       │  app runs) │            │                │                │                     │
       └────────────┘            └────────────────┘                └─────────────────────┘
```

This contract sits **above** S3.2 (S3.2 owns the sandbox composition and enforcement; this contract sequences the calls in launch order) and **below** S10.1 (S10.1 owns the typed action FSM and adapter dispatch; this contract is what the `app.launch` adapter does internally). It binds horizontally to S12.1 (the EcosystemRuntime selection arrives via the app manifest), to S11.1 (the EcosystemRuntime adapters are themselves packages, and the app being launched is a package), to S2.4 (the post-launch health probe is a verification primitive), and to S8.2 (the per-group VkDevice partition is enforced by the GpuPolicy field in the composed SandboxProfile so a Wine or Waydroid launch cannot reach another group's GPU surface).

## 3. Vocabulary (closed enums)

All vocabularies in this section are closed. Adding a value is a versioned spec change. The orchestrator MUST reject manifest fields and request fields containing values outside the enum at parse time. None of these enums admits an `OPEN` or `OTHER` value; the intent is to make orchestration semantics fully mechanical and to make outcome reporting fully mechanical too.

### 3.1 `OrchestrationKind`

Closed enum, eight kinds. Each kind is a distinct combination of `(EcosystemRuntime, prefix-or-container reuse strategy, isolation strategy)`. The orchestrator selects exactly one kind per launch based on the resolved app manifest plus the composed `SandboxProfile`. The kind is recorded in evidence and is the primary label dimension for the launch metrics in §10.

```proto
enum OrchestrationKind {
  ORCHESTRATION_KIND_UNSPECIFIED = 0;

  // Direct ELF launch under a SUBPROCESS_FORK dispatch with the composed
  // sandbox profile applied to the child. No translation layer.
  // Bound EcosystemRuntime: RUNTIME_LINUX_NATIVE, RUNTIME_FLATPAK,
  // RUNTIME_SNAP, RUNTIME_DISTROBOX (when the container is already
  // running and the launch is a re-exec inside it).
  NATIVE_DIRECT_LAUNCH = 1;

  // Flatpak's bwrap-based runtime is invoked as a sibling sandbox
  // INSIDE the AIOS-composed sandbox. The S3.2 floor remains the
  // outer constitutional boundary; bwrap is the inner ecosystem layer.
  // Bound EcosystemRuntime: RUNTIME_FLATPAK.
  FLATPAK_FORK = 2;

  // AppImage extracted to a per-app private mount; the extracted
  // entrypoint is launched via NATIVE_DIRECT_LAUNCH semantics with
  // an additional read-only loop-mount for the AppImage payload.
  // Bound EcosystemRuntime: RUNTIME_APPIMAGE.
  APPIMAGE_EXTRACT = 3;

  // Wine/Proton prefix is created fresh per launch (or per app
  // depending on WinePrefixKind §3.3) and the Win32 EXE is launched
  // inside the prefix under the composed sandbox.
  // Bound EcosystemRuntime: RUNTIME_WINDOWS_PROTON.
  WINE_PREFIX_NEW = 4;

  // Wine/Proton prefix from a prior launch is reused; sandbox profile
  // is reapplied; the Win32 EXE is launched inside the existing prefix.
  // Bound EcosystemRuntime: RUNTIME_WINDOWS_PROTON.
  WINE_PREFIX_EXISTING = 5;

  // Waydroid LXC container is created fresh and the Android APK
  // entrypoint is launched inside it.
  // Bound EcosystemRuntime: RUNTIME_ANDROID_WAYDROID.
  WAYDROID_CONTAINER_NEW = 6;

  // Waydroid LXC container from a prior launch is reused; the APK
  // entrypoint is launched inside it.
  // Bound EcosystemRuntime: RUNTIME_ANDROID_WAYDROID.
  WAYDROID_CONTAINER_EXISTING = 7;

  // KVM guest is booted (or unpaused if previously suspended) and
  // the app entrypoint is invoked via the guest agent.
  // Bound EcosystemRuntime: RUNTIME_WINDOWS_VM, RUNTIME_ANDROID_VM_WITH_GMS,
  // RUNTIME_MACOS_VM.
  KVM_VM_BOOT = 8;
}
```

The enum is closed. Values not in this list are rejected by:

- the orchestrator's `Launch` RPC at parse time;
- the app manifest validator (S11.1 admit pipeline) when the app declares an OrchestrationKind preference;
- the launch metric label decoder.

The orchestrator is responsible for selecting an `OrchestrationKind` consistent with the manifest's `EcosystemRuntime`. The mapping table is exhaustive: every supported `(EcosystemRuntime, app_state)` pair maps to exactly one `OrchestrationKind`. The mapping is in §5.4.

### 3.2 `LaunchOutcome`

Closed enum, seven outcomes. The orchestrator emits exactly one `LaunchOutcome` per `app.launch` action. Failure outcomes carry a structured `LaunchFailureReason` (§4) so the L7 marketplace surface and the operator approval prompt can show the operator what went wrong without raw error text.

```proto
enum LaunchOutcome {
  LAUNCH_OUTCOME_UNSPECIFIED = 0;

  // App process started inside the sandbox; first health signal
  // received within the OrchestrationKind's launch budget (§6).
  LAUNCHED = 1;

  // The required EcosystemRuntime adapter package is not installed
  // or is in a non-ACTIVE state (per S11.1 §5 install pipeline).
  // The orchestrator does not auto-install runtimes during launch;
  // this is an operator decision via the marketplace surface.
  LAUNCH_FAILED_RUNTIME_MISSING = 2;

  // S3.2 ComposeProfile or ApplyProfile returned an error before
  // the runtime adapter was invoked. Common causes: policy disagrees
  // with manifest (CompositionError = POLICY_LOOSENED_BY_LOWER_SOURCE),
  // host capability lie (HOST_CAPABILITY_LIE), required backend
  // missing without approved fallback (REQUIRED_BACKEND_UNAVAILABLE),
  // ecosystem_runtime mismatch across sources (§20.2 of S3.2).
  LAUNCH_FAILED_SANDBOX_DENY = 3;

  // S8.2 GpuPolicy denied the requested GPU class for this launch
  // (e.g., the app requested GPU_FULL_3D but the per-group VkDevice
  // partition has no compute pipeline approval). The runtime adapter
  // refuses to start the process because GPU init would fail
  // catastrophically inside the sandbox.
  LAUNCH_FAILED_GPU_DENY = 4;

  // The composed NetworkPolicy denies an endpoint the runtime adapter
  // itself requires (e.g., Waydroid's session manager cannot reach
  // its own service inside the LXC container because the network
  // namespace is locked down too tightly). Distinct from per-app
  // network denials at runtime, which are not launch failures.
  LAUNCH_FAILED_NETWORK_DENY = 5;

  // Sandbox applied successfully and runtime adapter reported the
  // process started, but the post-launch verification probe (§7)
  // did not receive the app's expected health signal within the
  // verification window. The probe failure caused the launch to
  // be rolled back and the process terminated.
  LAUNCH_FAILED_VERIFY_FAIL = 6;

  // The launch did not reach LAUNCHED, LAUNCH_FAILED_*, or
  // LAUNCH_FAILED_VERIFY_FAIL within the OrchestrationKind's hard
  // timeout (§6). Cleanup is best-effort.
  TIMED_OUT = 7;
}
```

The enum is closed.

### 3.3 `WinePrefixKind`

Closed enum, three kinds. Selects the prefix isolation strategy for `RUNTIME_WINDOWS_PROTON` launches. Recorded in evidence and surfaces to the operator at install time (the marketplace surface displays the recipe's declared `WinePrefixKind` so the operator knows whether their save game state will persist or be wiped on next launch).

```proto
enum WinePrefixKind {
  WINE_PREFIX_KIND_UNSPECIFIED = 0;

  // Fresh prefix per launch. Default for transient apps and for the
  // first launch of any app with no existing prefix. Maps to
  // OrchestrationKind = WINE_PREFIX_NEW. Z: drive does NOT see the
  // host home (binds INV-011 — cross-group access forbidden — and
  // S3.2 §9.1 block_host_home = true). Registry is empty at start.
  PER_APP_FRESH = 1;

  // Persistent prefix scoped to a single app. Reused across launches.
  // Save game state, registry tweaks, and per-app DLL overrides
  // persist. Maps to OrchestrationKind = WINE_PREFIX_EXISTING after
  // the first launch creates the prefix. Z: drive isolation rule
  // remains: the prefix's Z: drive points to /aios/groups/<g>/users/
  // <u>/runtime/wine/<app_id>/zdrive — never to the host home.
  PER_APP_PERSISTENT = 2;

  // Shared prefix scoped to a single user (one prefix shared across
  // multiple apps that explicitly opt into sharing — for example a
  // Microsoft Office suite where Word, Excel, PowerPoint share a
  // single Office prefix). Requires explicit operator approval at
  // install time and is rejected if any participating app declares
  // a per-app fresh requirement. Z: drive scope = user, never group
  // or host. The shared prefix is still per-user, never per-group;
  // INV-011 binds.
  SHARED_PER_USER = 3;
}
```

The enum is closed. The default for a manifest with no declared `WinePrefixKind` is `PER_APP_FRESH` — the most-restrictive choice consistent with S3.2's default-deny discipline.

### 3.4 `WaydroidIsolationLevel`

Closed enum, three levels. Selects the container isolation strategy for `RUNTIME_ANDROID_WAYDROID` launches. The level is recorded in evidence and surfaces to the operator at install time.

```proto
enum WaydroidIsolationLevel {
  WAYDROID_ISOLATION_LEVEL_UNSPECIFIED = 0;

  // Each app runs in its own Waydroid container. Highest isolation;
  // highest cold-launch cost. Container data lives at
  // /aios/groups/<g>/users/<u>/runtime/waydroid/<app_id>/data and
  // is invisible to every other Waydroid app. Default for any app
  // that requests microphone, camera, or location capabilities.
  PER_APP = 1;

  // All Android apps for a single user share one Waydroid container
  // but each app's data directory is bind-mounted as a per-app
  // overlay so cross-app filesystem access remains denied at the
  // sandbox layer. Lower cold-launch cost; explicit operator opt-in
  // required at install time.
  PER_USER = 2;

  // All Android apps for a single AIOS group share one Waydroid
  // container with the same per-app overlay discipline as PER_USER.
  // Requires explicit operator approval and is rejected if the
  // group has more than one human user with conflicting privacy
  // requirements. INV-011 still binds: the container's namespace
  // is the group's namespace, never another group's.
  PER_GROUP = 3;
}
```

The enum is closed. The default for a manifest with no declared `WaydroidIsolationLevel` is `PER_APP` — the most-restrictive choice.

### 3.5 `VMFallbackKind`

Closed enum, four kinds. Selects the documented justification for using a VM-based EcosystemRuntime (`RUNTIME_WINDOWS_VM`, `RUNTIME_ANDROID_VM_WITH_GMS`, `RUNTIME_MACOS_VM`). The kind is recorded in evidence and surfaces to the operator at install time (the marketplace surface displays "this app needs a VM because <reason>" so the operator understands the resource implications).

```proto
enum VMFallbackKind {
  VM_FALLBACK_KIND_UNSPECIFIED = 0;

  // Game or app ships kernel-level anti-cheat (Easy Anti-Cheat,
  // BattlEye in kernel mode, Vanguard) that refuses to run under
  // Wine/Proton or detects the translation layer and bans the
  // account. The honest answer is a full Windows VM; AIOS does
  // not falsify Windows-native fingerprints (S12.1 §9.7).
  WINDOWS_ANTI_CHEAT = 1;

  // App requires a Windows kernel driver (e.g., specialty hardware
  // control, legacy printer driver, kernel-mode debugger). Wine
  // does not implement Windows kernel APIs; only a real Windows
  // kernel can load these drivers.
  KERNEL_DRIVER = 2;

  // App requires hardware emulation that is impractical to expose
  // through Wine or Waydroid (e.g., a macOS app expecting an
  // Apple T2 chip; an Android image expecting Play Integrity
  // hardware attestation; a Windows app expecting TPM 2.0 with
  // specific PCR values). The VM provides the emulated hardware
  // surface; AIOS discloses that the emulation is best-effort.
  EXOTIC_HARDWARE = 3;

  // The operator explicitly chose VM fallback even though a Wine
  // or Waydroid path exists. Recorded so the choice is auditable;
  // the marketplace surface shows the operator the alternative
  // they overrode and the resource cost they accepted.
  OPERATOR_FORCED = 4;
}
```

The enum is closed.

## 4. Proto IDL

```proto
syntax = "proto3";
package aios.compatruntime.v1alpha1;

import "google/protobuf/duration.proto";
import "google/protobuf/timestamp.proto";

// External imports — closed-typed references; this contract does not
// redefine these vocabularies.
//
//   aios.appcompat.v1alpha1.EcosystemRuntime          (S12.1)
//   aios.appcompat.v1alpha1.EcosystemHonestyClass     (S12.1)
//   aios.sandbox.v1alpha1.SandboxProfile               (S3.2)
//   aios.sandbox.v1alpha1.CompositionError             (S3.2)
//   aios.runtime.v1alpha1.ActionEnvelope               (S0.1 / S10.1)
//   aios.evidence.v1alpha1.RecordType                  (S3.1)

// ============================================================================
// Service
// ============================================================================

service CompatibilityRuntime {
  // Launch an installed app. The action_id MUST belong to an envelope in
  // EXECUTING state (S0.1) for typed action `app.launch`. The runtime is
  // not allowed to be invoked outside that lifecycle position.
  rpc Launch(LaunchRequest) returns (LaunchResponse);

  // Cancel an in-flight launch (only valid before LaunchOutcome is set).
  // Emits APP_LAUNCH_FAILED with reason CANCELLED. Cleanup is best-effort
  // for partially started runtime adapters.
  rpc CancelLaunch(CancelLaunchRequest) returns (CancelLaunchResponse);

  // Query orchestrator state for a given action_id. Read-only.
  rpc GetLaunchStatus(GetLaunchStatusRequest) returns (GetLaunchStatusResponse);

  // Engine info: orchestrator version, schema version, supported
  // OrchestrationKinds in the running build (subset of the closed enum
  // gated by which EcosystemRuntime adapters are installed).
  rpc GetOrchestratorInfo(GetOrchestratorInfoRequest) returns (GetOrchestratorInfoResponse);
}

// ============================================================================
// Request/response shapes
// ============================================================================

message LaunchRequest {
  string action_id = 1;                    // act_<ulid26>; from S0.1 envelope
  string app_id = 2;                       // app:<canonical>; from /aios/.../apps/
  string subject_canonical_id = 3;         // S5.1 identity
  bool is_ai = 4;                          // mirrors envelope subject classification
  bool is_recovery_mode = 5;               // mirrors envelope subject classification

  // Optional caller hints. These hints are ADVISORY only — the
  // orchestrator validates each against the manifest's declared
  // EcosystemRuntime and rejects any hint that would change the
  // bound runtime (e.g. a hint requesting RUNTIME_WINDOWS_VM for an
  // app whose manifest declares RUNTIME_WINDOWS_PROTON without an
  // accompanying VMFallbackKind = OPERATOR_FORCED override).
  OrchestrationHint hint = 10;
}

message OrchestrationHint {
  // Operator's explicit OrchestrationKind preference. The orchestrator
  // MAY accept this if it is consistent with the manifest's
  // EcosystemRuntime and the policy floor; otherwise rejected at
  // launch time with LaunchOutcome = LAUNCH_FAILED_SANDBOX_DENY.
  OrchestrationKind preferred_kind = 1;

  // For Wine launches: explicit prefix kind override.
  WinePrefixKind wine_prefix_kind = 2;

  // For Waydroid launches: explicit isolation level override.
  WaydroidIsolationLevel waydroid_isolation_level = 3;

  // For VM launches: explicit fallback kind. Required when the
  // operator force-routes a Wine-capable app into a VM.
  VMFallbackKind vm_fallback_kind = 4;
}

message LaunchResponse {
  LaunchOutcome outcome = 1;
  OrchestrationKind chosen_kind = 2;
  string sandbox_profile_id = 3;            // prof_<hex>; from S3.2
  string runtime_adapter_id = 4;            // adapter:proton:9.0.0 etc.
  uint32 launched_pid = 5;                  // 0 if outcome != LAUNCHED
  google.protobuf.Duration time_to_launched = 6;
  LaunchFailureReason failure = 7;          // populated on LAUNCH_FAILED_*
  repeated string evidence_receipt_ids = 8;
}

message LaunchFailureReason {
  // High-level cause. Closed enum.
  FailureCategory category = 1;

  // Sub-reason. Closed catalog id (orchestrator publishes the
  // catalog versioned alongside this spec). Free-form text in
  // failure paths is forbidden; the operator-facing message is
  // looked up from the catalog id.
  string reason_id = 2;

  // The composition error returned by S3.2 when category =
  // SANDBOX_COMPOSITION_FAILED. Empty otherwise.
  string sandbox_error_code = 3;

  // The verification probe failure code (S2.4 closed enum) when
  // category = VERIFICATION_FAILED. Empty otherwise.
  string verification_error_code = 4;
}

enum FailureCategory {
  FAILURE_CATEGORY_UNSPECIFIED = 0;
  RUNTIME_ADAPTER_MISSING = 1;
  SANDBOX_COMPOSITION_FAILED = 2;
  SANDBOX_APPLICATION_FAILED = 3;
  GPU_POLICY_DENIED = 4;
  NETWORK_POLICY_DENIED = 5;
  RUNTIME_ADAPTER_RETURNED_ERROR = 6;
  VERIFICATION_FAILED = 7;
  TIMED_OUT_BUDGET_EXCEEDED = 8;
  CANCELLED = 9;
  ECOSYSTEM_RUNTIME_MISMATCH = 10;
}

message CancelLaunchRequest { string action_id = 1; string reason = 2; }
message CancelLaunchResponse { bool cancelled = 1; string evidence_receipt_id = 2; }

message GetLaunchStatusRequest { string action_id = 1; }
message GetLaunchStatusResponse {
  LaunchPhase phase = 1;
  google.protobuf.Timestamp phase_entered_at = 2;
  OrchestrationKind chosen_kind = 3;
  string sandbox_profile_id = 4;
  string runtime_adapter_id = 5;
}

enum LaunchPhase {
  LAUNCH_PHASE_UNSPECIFIED = 0;
  RESOLVING_MANIFEST = 1;
  COMPOSING_SANDBOX = 2;
  APPLYING_SANDBOX = 3;
  INVOKING_RUNTIME_ADAPTER = 4;
  STARTING_PROCESS = 5;
  WAITING_FOR_VERIFICATION = 6;
  COMPLETED = 7;
}

message GetOrchestratorInfoRequest {}
message GetOrchestratorInfoResponse {
  string orchestrator_version = 1;
  string schema_version = 2;
  repeated OrchestrationKind supported_kinds = 3;
  repeated string installed_runtime_adapter_ids = 4;
}
```

## 5. Orchestration flow

This section is the operational heart of the contract. Every launch executes the same eight-step sequence in the same order. Each step has explicit success and failure exits. The orchestrator never skips a step on the success path; it never proceeds past a step that returned a failure.

### 5.1 Step A — manifest resolution

Inputs: `action_id`, `app_id`, `subject_canonical_id`. Outputs: the resolved app manifest (a `PackageManifest` per S11.1) and the manifest's declared `EcosystemRuntime`.

The orchestrator reads the installed app's manifest from the AIOS-FS path resolved by S4.1 namespace rules:

```text
/aios/groups/<g>/users/<u>/apps/<app_id>/manifest      (per-user installs)
/aios/groups/<g>/apps/<app_id>/manifest                (per-group installs)
/aios/system/apps/<app_id>/manifest                    (system-scoped; INV-013 binds)
```

If the manifest is absent, malformed, or its signature does not validate against the publisher catalog (S11.1 §3.1), Step A fails with `LAUNCH_FAILED_SANDBOX_DENY` and `FailureCategory = ECOSYSTEM_RUNTIME_MISMATCH` (because the orchestrator cannot proceed without a valid declared `EcosystemRuntime`). The launch is logged as `APP_LAUNCH_FAILED`.

If the manifest declares `EcosystemRuntime = RUNTIME_UNSPECIFIED` (forbidden by S3.2 §20.1 for fully-composed profiles and forbidden here for any installed manifest), Step A fails with the same category. `ORCHESTRATION_KIND_MISMATCH_REJECTED` (FOREVER) is emitted because an unspecified runtime in an installed manifest is a tamper signal.

### 5.2 Step B — composition input assembly

Inputs: the resolved manifest, the action envelope, the policy decision id (already attached by S10.1 EvaluatePolicy). Outputs: a fully populated `aios.sandbox.v1alpha1.CompositionInputs` ready for S3.2.

The orchestrator constructs the five composition sources as follows:

1. **`adapter_default`** — the `EcosystemRuntime` adapter package's published default profile. The adapter id is derived from the manifest's `EcosystemRuntime` value via the adapter registry (S11.1 §3.4): `RUNTIME_WINDOWS_PROTON` → `adapter:proton:<version>`, `RUNTIME_ANDROID_WAYDROID` → `adapter:waydroid:<version>`, etc. The adapter's published default profile is fetched and its `ecosystem_runtime` field (S3.2 §20) is verified to match the manifest's declared runtime. Mismatch fails with `ECOSYSTEM_RUNTIME_MISMATCH`.

2. **`app_manifest`** — the manifest's `required` SandboxProfile section. The manifest's declared `ecosystem_runtime` is checked again against the adapter's; consistent values pass to S3.2.

3. **`user_request`** — the optional `OrchestrationHint` from the LaunchRequest, projected onto the SandboxProfile shape. Per S3.2 §3 (UserRequestHint discipline) the orchestrator drops permissive hints silently and forwards only restrictive hints. An OrchestrationHint that names a different `EcosystemRuntime` than the manifest is rejected at this step before reaching S3.2.

4. **`policy_required`** — fetched from S2.3 by the policy decision id attached to the envelope. The policy bundle's per-app constraints already include any group-scoped or organization-scoped overrides for the app's runtime selection.

5. **`runtime_safety_floor`** — the signed floor bundle currently loaded by the orchestrator process. Per S3.2 §5.2 the orchestrator selects the floor variant matching the subject class (`is_ai`, `is_recovery_mode`).

The orchestrator also attaches the host capability snapshot (S3.2 §6.1) and the action context (`action_id`, `subject_canonical_id`, `is_ai`, `is_recovery_mode`).

### 5.3 Step C — sandbox composition

Inputs: the assembled `CompositionInputs`. Outputs: a composed `SandboxProfile` whose `ecosystem_runtime` field matches the manifest's declaration, or a `CompositionError`.

The orchestrator calls `aios.sandbox.v1alpha1.SandboxComposer.ComposeProfile`. S3.2 owns the merge algorithm (S3.2 §5) and the floor enforcement (S3.2 §5.4). On success the orchestrator obtains a `SandboxProfile` with a content-addressed `profile_id`. On failure the orchestrator immediately emits `APP_LAUNCH_FAILED` with `FailureCategory = SANDBOX_COMPOSITION_FAILED` and the `CompositionError.code` propagated to the operator-facing message. The orchestrator does NOT retry composition with a relaxed input set; the composition error is the answer.

### 5.4 Step D — OrchestrationKind selection

Inputs: the composed `SandboxProfile`, the resolved manifest, the optional `OrchestrationHint`. Outputs: exactly one `OrchestrationKind` from §3.1.

The selection is purely mechanical. The mapping is exhaustive; every reachable `(EcosystemRuntime, app_state, hint)` tuple maps to one kind:

| `EcosystemRuntime`            | App state                                                                            | `OrchestrationKind`           |
| ----------------------------- | ------------------------------------------------------------------------------------ | ----------------------------- |
| `RUNTIME_LINUX_NATIVE`        | always                                                                               | `NATIVE_DIRECT_LAUNCH`        |
| `RUNTIME_FLATPAK`             | first launch or runtime not currently bound                                          | `FLATPAK_FORK`                |
| `RUNTIME_FLATPAK`             | re-exec inside an already-mounted runtime                                            | `NATIVE_DIRECT_LAUNCH`        |
| `RUNTIME_APPIMAGE`            | always                                                                               | `APPIMAGE_EXTRACT`            |
| `RUNTIME_SNAP`                | always                                                                               | `NATIVE_DIRECT_LAUNCH`        |
| `RUNTIME_DISTROBOX`           | container already running                                                            | `NATIVE_DIRECT_LAUNCH`        |
| `RUNTIME_DISTROBOX`           | container cold                                                                       | rejected (not a launch path)  |
| `RUNTIME_WINDOWS_PROTON`      | no prior prefix OR `WinePrefixKind = PER_APP_FRESH`                                  | `WINE_PREFIX_NEW`             |
| `RUNTIME_WINDOWS_PROTON`      | prior prefix exists AND `WinePrefixKind` ∈ {`PER_APP_PERSISTENT`, `SHARED_PER_USER`} | `WINE_PREFIX_EXISTING`        |
| `RUNTIME_ANDROID_WAYDROID`    | container cold                                                                       | `WAYDROID_CONTAINER_NEW`      |
| `RUNTIME_ANDROID_WAYDROID`    | container warm                                                                       | `WAYDROID_CONTAINER_EXISTING` |
| `RUNTIME_WINDOWS_VM`          | always                                                                               | `KVM_VM_BOOT`                 |
| `RUNTIME_ANDROID_VM_WITH_GMS` | always                                                                               | `KVM_VM_BOOT`                 |
| `RUNTIME_MACOS_DARLING`       | always                                                                               | `NATIVE_DIRECT_LAUNCH`        |
| `RUNTIME_MACOS_VM`            | always                                                                               | `KVM_VM_BOOT`                 |
| `RUNTIME_REMOTE_APPLE_BRIDGE` | always                                                                               | rejected (not a local launch) |

`RUNTIME_REMOTE_APPLE_BRIDGE` and `RUNTIME_DISTROBOX` cold-start cases are rejected because they are not local app-launch operations; they are bridge or container-bring-up actions handled by separate typed actions (`bridge.connect_apple` and `container.distrobox_enter`, respectively, owned by L8 / L6 sub-specs to be written).

If the operator's `OrchestrationHint.preferred_kind` disagrees with the row selected by the table, the launch fails with `LAUNCH_FAILED_SANDBOX_DENY` and `FailureCategory = ECOSYSTEM_RUNTIME_MISMATCH`. The hint is allowed to refine within a row (e.g., select between `WINE_PREFIX_NEW` and `WINE_PREFIX_EXISTING` when both are reachable for `PER_APP_PERSISTENT`) but not to cross rows.

### 5.5 Step E — sandbox application

Inputs: the composed `SandboxProfile`, the `action_id`. Outputs: an applied profile with `ProfileLifecycle = APPLIED` (S3.2 §7.1) and a target process group ready to receive the runtime adapter's first child fork.

The orchestrator calls `aios.sandbox.v1alpha1.SandboxComposer.ApplyProfile` with `target_pid = 0` (the runtime adapter has not yet been invoked; the sandbox is applied to the orchestrator's about-to-be-spawned subprocess). On success the orchestrator obtains the `applied_profile_id` and the `evidence_receipt_id` for `SANDBOX_PROFILE_APPLIED`. On failure (kernel-level enforcement could not be loaded, e.g., Landlock returned an error mid-apply) the orchestrator emits `APP_LAUNCH_FAILED` with `FailureCategory = SANDBOX_APPLICATION_FAILED`.

### 5.6 Step F — runtime adapter invocation

Inputs: the applied SandboxProfile, the `OrchestrationKind`, the manifest. Outputs: a started app process whose pid is `launched_pid`, or a runtime adapter error.

The orchestrator calls the EcosystemRuntime adapter's typed `runtime.<adapter_name>.launch_app` action via S10.1 internal dispatch. The adapter receives:

- the applied SandboxProfile (so the adapter cannot accidentally fork a child outside the sandbox);
- the `OrchestrationKind` (so the adapter knows whether to create a fresh prefix, reuse an existing prefix, etc.);
- the manifest's declared `WinePrefixKind` / `WaydroidIsolationLevel` / `VMFallbackKind` where applicable;
- the path of the per-orchestration scratch directory (e.g. `/aios/groups/<g>/users/<u>/runtime/wine/<app_id>/prefix` for Wine, `/aios/groups/<g>/users/<u>/runtime/waydroid/<app_id>/data` for Waydroid, `/aios/groups/<g>/users/<u>/runtime/vm/<app_id>/disk` for VMs);
- the entrypoint identifier from the manifest (binary path for native, EXE name for Wine, package name for Android, command-line for VM agent).

The adapter starts the app process inside the sandbox boundary. The orchestrator does NOT receive the raw process handle; the runtime adapter owns the process lifetime. The orchestrator only needs the pid for evidence and the verification probe target.

If the adapter returns an error (Wine prefix initialization failed, Waydroid LXC start failed, KVM guest agent unreachable), the orchestrator emits `APP_LAUNCH_FAILED` with `FailureCategory = RUNTIME_ADAPTER_RETURNED_ERROR` and the adapter's structured error code is the `reason_id`.

### 5.7 Step G — verification probe

Inputs: `launched_pid`, the manifest's declared `verification_probe` (a typed S2.4 probe specification). Outputs: probe passed (proceed to LAUNCHED) or probe failed (roll back).

The orchestrator instantiates the S2.4 verification probe declared in the manifest. The probe is a closed-typed primitive (per S2.4) selected from a closed catalog: `process_alive`, `port_listening`, `unix_socket_listening`, `dbus_name_acquired`, `wayland_surface_visible`, `manifest_health_endpoint`. The probe runs with a timeout equal to the orchestration kind's verification window (§6).

If the probe passes within the window, the launch transitions to `LAUNCHED`. If the probe fails or times out, the orchestrator calls `aios.sandbox.v1alpha1.SandboxComposer.RevokeProfile` to tear down the sandbox (which terminates the app process via the kernel-level enforcement boundary) and emits `APP_LAUNCH_FAILED` with `FailureCategory = VERIFICATION_FAILED`. The probe failure is included in the failure record.

### 5.8 Step H — outcome emission

Inputs: the outcome decided by Step G. Outputs: `APP_LAUNCH_SUCCEEDED` (on `LAUNCHED`) or `APP_LAUNCH_FAILED` (on any other outcome), plus the per-orchestration-kind launch metric increment.

The orchestrator writes the outcome record. The action envelope transitions out of `EXECUTING` per S0.1: `LAUNCHED` → `SUCCEEDED`, anything else → `FAILED`. The L7 marketplace surface (the operator's notification surface) receives the structured outcome and renders a localized message from the failure catalog id. The operator never sees raw error text.

## 6. Per-OrchestrationKind launch budgets

Every launch has a hard timeout per `OrchestrationKind`. A launch that exceeds its hard timeout transitions to `TIMED_OUT` with cleanup. The budgets below are honest worst-case figures for first-launch cold paths on reasonable hardware (a 2024-class workstation with NVMe storage). Re-launch (warm) figures are typically 3–10× faster but are not contractual. The verification window is the maximum time the orchestrator will wait for the post-launch probe (§5.7) before declaring `LAUNCH_FAILED_VERIFY_FAIL`.

| `OrchestrationKind`           | Cold-launch budget (p95)  | Hard timeout | Verification window |
| ----------------------------- | ------------------------- | ------------ | ------------------- |
| `NATIVE_DIRECT_LAUNCH`        | < 100 ms                  | 1 s          | 500 ms              |
| `FLATPAK_FORK`                | < 500 ms                  | 5 s          | 2 s                 |
| `APPIMAGE_EXTRACT`            | < 1 s (extract dominated) | 10 s         | 3 s                 |
| `WINE_PREFIX_NEW`             | < 5 s                     | 30 s         | 10 s                |
| `WINE_PREFIX_EXISTING`        | < 1 s                     | 10 s         | 5 s                 |
| `WAYDROID_CONTAINER_NEW`      | < 10 s                    | 60 s         | 20 s                |
| `WAYDROID_CONTAINER_EXISTING` | < 2 s                     | 15 s         | 5 s                 |
| `KVM_VM_BOOT`                 | < 60 s                    | 180 s        | 30 s                |

These budgets bind the launch metric histograms (§10) and the operator-facing progress UI in the L7 marketplace surface. A persistent regression beyond budget is itself an operational signal — the orchestrator emits a `launch_budget_exceeded` counter increment per kind, which is a Prometheus alert dimension.

The budgets exclude time spent in S5.3 approval (if the action requires human approval) and time spent in S2.3 policy evaluation; those are upstream of Step A. The budget clock starts at Step E (sandbox application) for `NATIVE_DIRECT_LAUNCH` and at Step F (runtime adapter invocation) for every other kind, because Step A through Step D should be sub-millisecond for a well-cached manifest and policy bundle.

## 7. Wine prefix isolation discipline

The `WinePrefixKind` (§3.3) value selects the prefix layout but every prefix shares the following constitutional rules. These rules are enforced by the Wine runtime adapter under the SandboxProfile applied in Step E and reinforced by the floor at S3.2 §9.1.

### 7.1 Z: drive scope

Wine's traditional behaviour is to map `Z:` to the host root (`/`). AIOS rejects this. The Wine runtime adapter rewrites the prefix's `dosdevices/z::` symlink at prefix creation time (or at every launch for `PER_APP_FRESH`) to point to the per-app scratch directory:

```text
/aios/groups/<g>/users/<u>/runtime/wine/<app_id>/zdrive
```

This directory is empty by default and is the only host-visible path the prefix can reach via the `Z:` drive letter. The host home (`/home/<u>` or `/aios/groups/<g>/users/<u>/home`) is NOT reachable through `Z:` regardless of how the Win32 binary tries to walk the path. The S3.2 floor's `block_host_home = true` for `CompatibilityKind = WINE_PROTON` is what enforces the underlying filesystem deny; the adapter's symlink rewrite is the cooperating ecosystem-side discipline.

This rule binds **INV-011** (cross-group access forbidden by default) — a Wine prefix cannot reach `/aios/groups/<other_g>/...` regardless of operator request, manifest declaration, or adapter default. The composer's filesystem floor strips any cross-group path from the merged `allow_read` and `allow_write` sets before applying.

### 7.2 Registry isolation

The Wine registry (`system.reg`, `user.reg`, `userdef.reg` inside the prefix) is scoped to the prefix. There is no shared Wine registry across prefixes. `PER_APP_FRESH` always starts with the bundled empty-registry skeleton; `PER_APP_PERSISTENT` and `SHARED_PER_USER` retain registry state across launches but only within the prefix's scope.

### 7.3 Brokered clipboard

The Wine prefix's clipboard is brokered through the AIOS portal (the same portal L7 uses for native apps). The Wine runtime adapter installs the AIOS clipboard bridge inside the prefix at creation time. Direct access to the host display server's clipboard from a Win32 process is denied at the sandbox layer by the SandboxProfile's `compatibility.wine_proton.portal_clipboard = true` floor.

### 7.4 Networking inside the prefix

The Wine runtime does not provide an additional network namespace; the SandboxProfile's `network` block applied in Step E governs all outbound traffic from the prefix. A Win32 binary's WinSock calls go through Wine's sock\_\*.dll into the Linux kernel's network namespace, which is the sandboxed namespace configured by S3.2.

### 7.5 GPU access inside the prefix

The Wine runtime exposes Direct3D / Vulkan via DXVK / VKD3D-Proton. The underlying Vulkan device exposed to the prefix is the per-group `VkDevice` instance from S8.2, never a global GPU device. A Wine prefix cannot break out of its `VkDevice` to reach another group's compute pipeline; INV-011 is enforced at the GPU layer by S8.2's per-group device partition.

## 8. Waydroid isolation discipline

The `WaydroidIsolationLevel` (§3.4) value selects the container layout but every Waydroid launch shares the following constitutional rules. These rules are enforced by the Waydroid runtime adapter under the SandboxProfile applied in Step E and reinforced by the floor at S3.2 §9.2.

### 8.1 Per-app data directory

Each app's persistent data is written to:

```text
/aios/groups/<g>/users/<u>/runtime/waydroid/<app_id>/data
```

For `PER_APP` isolation this path is the entire data root the app sees (mapped to `/data/data/<package>` inside the container's Android filesystem view). For `PER_USER` and `PER_GROUP` isolation the container shares its Android system partition across apps but each app's `/data/data/<package>` is a distinct bind mount onto the per-app directory above. Cross-app filesystem reads inside the container are denied by the AIOS sandbox layer below the Waydroid bind mounts; the Android permission model's loose interpretations of "shared external storage" do not apply.

### 8.2 Brokered clipboard and portals

The Android clipboard is brokered through the AIOS portal. The Waydroid runtime adapter installs the AIOS clipboard bridge inside the container at creation time. Same discipline as §7.3.

The Android storage access framework (file picker, document provider) is brokered through the AIOS file portal. An Android app calling `Intent.ACTION_GET_CONTENT` is intercepted by the AIOS portal layer and shown the operator's chosen file rather than receiving a raw cross-app filesystem view.

### 8.3 No shared display memory

Each Waydroid container has its own Wayland display server isolated from the host's display server. The containers do not share GPU buffer pools across containers, and each container's GPU access is scoped to the per-group `VkDevice` (S8.2). A `PER_APP` container has its own display server; a `PER_USER` or `PER_GROUP` container shares a display server but each app's surfaces live in distinct compositor scopes within that server.

### 8.4 No Google services in `RUNTIME_ANDROID_WAYDROID`

Per S12.1 §3.1, `RUNTIME_ANDROID_WAYDROID` is `PARTIALLY_SUPPORTED` precisely because Google Play Services are absent. The Waydroid runtime adapter does not ship GMS; the absence is honestly disclosed at install time (`EcosystemHonestyClass = PARTIALLY_SUPPORTED`). Apps that require GMS are routed to `RUNTIME_ANDROID_VM_WITH_GMS` (which uses `OrchestrationKind = KVM_VM_BOOT`, not Waydroid).

### 8.5 INV-011 binding

The Waydroid container's namespace is the AIOS group's namespace. A Waydroid app cannot reach `/aios/groups/<other_g>/...` because the LXC namespace is rooted at `/aios/groups/<g>/...` and cross-group paths do not resolve. The composer enforces this at sandbox application time; the LXC adapter has no path that would lift the constraint.

## 9. KVM_VM_BOOT discipline

The `KVM_VM_BOOT` orchestration kind covers `RUNTIME_WINDOWS_VM`, `RUNTIME_ANDROID_VM_WITH_GMS`, and `RUNTIME_MACOS_VM` launches. The `VMFallbackKind` (§3.5) records the documented justification.

### 9.1 Ephemeral vs persistent VMs

Two VM lifetime models are supported:

- **Ephemeral VM** — the VM is fresh on every launch. The base disk image is read-only and the VM's running disk is a copy-on-write overlay deleted on shutdown. Used for apps where state persistence is undesirable (e.g., a one-shot Windows installer the operator wants to run without long-term residue). Default for any VM launch where the manifest does not declare a persistent VM disk.

- **Persistent VM** — the VM has an explicit storage share (per S3.2 §9.3 `VmStorageShare`) backed by a host directory under `/aios/groups/<g>/users/<u>/runtime/vm/<app_id>/disk`. Used for apps where state must persist (e.g., a Windows-only CAD application with project files). Requires explicit operator approval at install time; the marketplace surface displays the persistent disk size and location.

### 9.2 No automounted host directories

The VM gets exactly the storage shares declared in the manifest. There is no virt-manager-style "share my home directory with the guest" affordance. If the operator wants to move a file into the VM, the path is explicit: the AIOS file portal mediates a one-shot copy into the persistent disk's designated drop directory. This rule is enforced by the KVM runtime adapter rejecting any storage-share entry not in the manifest.

### 9.3 Networking

The VM's network is bridged through the host's sandboxed network namespace (S3.2 NetworkPolicy applied in Step E). The VM cannot reach endpoints denied at the AIOS NetworkPolicy layer regardless of what the guest OS believes about its own networking. The VM never has direct host network access; the bridge is unidirectional from the guest's perspective and constrained by the composed allow list.

### 9.4 Evidence bridge

The VM's stdout / stderr / structured event stream is captured via the KVM guest agent and forwarded into the host's evidence log. The S3.2 `evidence_bridge = true` floor for `VM_FALLBACK` profiles ensures that VM-internal events are observable to the host's audit subjects without granting the VM raw access to the host's log.

### 9.5 GPU access

A VM that needs GPU access (e.g., a Windows game in `RUNTIME_WINDOWS_VM`) receives a virtio-gpu surface or a VFIO-passthrough device per the S8.2 GpuPolicy. VFIO passthrough is gated behind the `gpu_capability_class = GPU_FULL_3D_PASSTHROUGH` capability and is disabled by default. The GpuPolicy floor (S8.2 §19) enforces per-group `VkDevice` isolation even for passthrough; cross-group GPU contention is denied at the VFIO assignment layer.

## 10. Telemetry contract

All metrics use bounded label cardinality. `subject_canonical_id`, `app_id`, `prefix_path`, `container_id`, `vm_image_id`, and `action_id` are NEVER labels; they appear in evidence records only.

| Metric                             | Type      | Labels (closed sets)                                                                                                            |
| ---------------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `app_launch_total`                 | counter   | `outcome` (7-value enum), `orchestration_kind` (8-value enum), `ecosystem_runtime` (12-value enum)                              |
| `app_launch_duration_seconds`      | histogram | `orchestration_kind`, `outcome` (success-only and failure-only sub-distributions)                                               |
| `app_launch_budget_exceeded_total` | counter   | `orchestration_kind`                                                                                                            |
| `app_launch_phase_seconds`         | histogram | `orchestration_kind`, `phase` (`LaunchPhase` 7-value enum)                                                                      |
| `app_wine_prefix_created_total`    | counter   | `wine_prefix_kind` (3-value enum)                                                                                               |
| `app_waydroid_container_started`   | counter   | `waydroid_isolation_level` (3-value enum), `cold_start` (`true`/`false`)                                                        |
| `app_kvm_vm_booted_total`          | counter   | `vm_fallback_kind` (4-value enum), `ephemeral` (`true`/`false`)                                                                 |
| `app_runtime_breakout_total`       | counter   | `orchestration_kind`, `attempt_class` (closed: `WINE_BREAKOUT` / `WAYDROID_BREAKOUT` / `KVM_ESCAPE` / `ORCHESTRATION_MISMATCH`) |
| `app_orchestration_mismatch_total` | counter   | `mismatch_class` (closed: `MANIFEST_VS_ADAPTER` / `MANIFEST_VS_POLICY` / `HINT_VS_MANIFEST` / `COMPOSED_VS_MANIFEST`)           |

Cardinality budget: ≤ 200 active label tuples per metric. The product `outcome × orchestration_kind × ecosystem_runtime = 7 × 8 × 12 = 672` exceeds the budget for `app_launch_total`; the orchestrator collapses the `ecosystem_runtime` dimension to its parent `orchestration_kind` for the active metric and retains the full cross-product only in evidence records (where there is no cardinality budget). The other metrics are well under budget.

## 11. Evidence record types (queued for S3.1 Wave 8)

The following ten record types are queued for the S3.1 RecordType closed vocabulary. This contract does NOT modify S3.1 — the orchestrator integrates these in Wave 8.

| Record type                            | Trigger                                                                                                                                                                                                                                                                                                                                                                            | Retention class |
| -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------- |
| `APP_LAUNCH_STARTED`                   | `app.launch` action transitioned to `executing`; orchestrator entered Step A.                                                                                                                                                                                                                                                                                                      | STANDARD_24M    |
| `APP_LAUNCH_SUCCEEDED`                 | Step G passed; `LaunchOutcome = LAUNCHED`; envelope transitioned to `succeeded`.                                                                                                                                                                                                                                                                                                   | STANDARD_24M    |
| `APP_LAUNCH_FAILED`                    | Any `LaunchOutcome` ≠ `LAUNCHED`; envelope transitioned to `failed`. Carries `FailureCategory` and `reason_id`.                                                                                                                                                                                                                                                                    | EXTENDED_60M    |
| `WINE_PREFIX_CREATED`                  | A new Wine prefix was created (Step F under `WINE_PREFIX_NEW`). Carries `WinePrefixKind` and prefix path.                                                                                                                                                                                                                                                                          | STANDARD_24M    |
| `WINE_PREFIX_BREAKOUT_ATTEMPTED`       | The kernel-level sandbox enforcer (seccomp violation, ptrace of host process, attempted access to `/aios/system/...`) caught a Win32 binary trying to escape its prefix. Mirrors S12.1 §9.2 with this contract's launch context.                                                                                                                                                   | FOREVER         |
| `WAYDROID_CONTAINER_STARTED`           | A Waydroid container started (Step F under `WAYDROID_CONTAINER_NEW` or `WAYDROID_CONTAINER_EXISTING`). Carries `WaydroidIsolationLevel`.                                                                                                                                                                                                                                           | STANDARD_24M    |
| `WAYDROID_ESCAPE_ATTEMPTED`            | The LXC namespace boundary or the AIOS group namespace caught a Waydroid-internal process trying to reach a forbidden host or cross-group path. Mirrors S12.1 §9.3 with this contract's launch context.                                                                                                                                                                            | FOREVER         |
| `KVM_VM_BOOTED`                        | A KVM guest reached the guest agent's "ready" handshake (Step F under `KVM_VM_BOOT`). Carries `VMFallbackKind` and `ephemeral` flag.                                                                                                                                                                                                                                               | STANDARD_24M    |
| `KVM_VM_TERMINATED`                    | A KVM guest was shut down (operator request, app exit, or revoke-profile cascade). Carries termination reason.                                                                                                                                                                                                                                                                     | STANDARD_24M    |
| `ORCHESTRATION_KIND_MISMATCH_REJECTED` | The selected `OrchestrationKind` disagrees with the manifest's declared `EcosystemRuntime`, with the policy required value, or with an apply-time check (S3.2 §20.3). Constitutional invariant: cross-source ecosystem disagreement is never silently resolved. Mirrors the spirit of `APP_ECOSYSTEM_RUNTIME_BREAKOUT_ATTEMPTED` (S12.1) with this contract's orchestration scope. | FOREVER         |

Each record carries:

- `action_id`, `app_id`, `subject_canonical_id`;
- `orchestration_kind`, `ecosystem_runtime`, `wine_prefix_kind` / `waydroid_isolation_level` / `vm_fallback_kind` as relevant;
- `sandbox_profile_id` (the applied profile under which the launch ran);
- `runtime_adapter_id` and adapter version;
- `launch_phase` at the time of the record (for failure records);
- `failure.category` and `failure.reason_id` for `APP_LAUNCH_FAILED` records;
- redacted launch summary (no raw stdout, no raw secrets, no raw network payloads — INV-015 binds).

## 12. Adversarial robustness

This section enumerates the named adversaries this contract addresses and how it addresses each one mechanically.

### 12.1 Wine prefix breakout

**Adversary:** a Win32 binary inside a Wine prefix tries to escape the prefix and reach the host filesystem, ptrace a host process, or read another group's namespace.

**Mitigation:** the prefix's process tree runs under the `SandboxProfile` applied in Step E. The S3.2 `block_host_home = true` floor for `CompatibilityKind = WINE_PROTON` (S3.2 §9.1) denies host home access at the kernel layer. INV-011 binds at the namespace layer (the prefix is rooted at `/aios/groups/<g>/...` and cross-group paths do not resolve). S8.2's per-group `VkDevice` partition (S8.2 §19) prevents the prefix from reaching another group's GPU surface. Any attempt — successful or not — emits `WINE_PREFIX_BREAKOUT_ATTEMPTED` (FOREVER) and immediately quarantines the app. The operator cannot un-quarantine without S5.3 approval and the FOREVER evidence record remains.

### 12.2 Waydroid container escape

**Adversary:** an Android app inside a Waydroid container tries to break out of the LXC namespace and access the host or another group.

**Mitigation:** the Waydroid container runs under the AIOS group namespace (S4.1) plus the SandboxProfile applied in Step E plus the per-group `VkDevice` (S8.2). A successful LXC escape lands the process inside the AIOS group namespace, not on the host filesystem; INV-011 still denies cross-group paths at the AIOS sandbox layer below LXC. `WAYDROID_ESCAPE_ATTEMPTED` (FOREVER) is emitted regardless of whether the escape penetrated the LXC layer.

### 12.3 KVM VM escape

**Adversary:** a process inside a KVM guest exploits a hypervisor or virtio bug to escape into the host kernel.

**Mitigation:** the VM is itself wrapped in the SandboxProfile applied in Step E. A successful KVM escape lands the qemu-system process inside that sandbox; the qemu binary cannot reach host paths denied by the composed FilesystemPolicy. The composer's `compatibility.vm_fallback.evidence_bridge = true` floor ensures that VM-internal anomalies are visible to the host; the hypervisor itself runs under seccomp profiles (S3.2 §6.2) that limit blast radius even on a successful escape. `WINE_PREFIX_BREAKOUT_ATTEMPTED` semantics apply analogously when the escape originates inside a Windows VM and reaches outside the qemu sandbox; the event is logged under `app_runtime_breakout_total{attempt_class="KVM_ESCAPE"}` and emits `ORCHESTRATION_KIND_MISMATCH_REJECTED` (FOREVER) when paired with a manifest that declared a non-VM EcosystemRuntime.

### 12.4 Orchestration substitution attack

**Adversary:** an app installer claims `RUNTIME_LINUX_NATIVE` at install time but the bundled binary is actually a Win32 EXE that, once approved, would be exec'd by an installer script that then invokes Wine to run it. Or: the installer claims `RUNTIME_WINDOWS_PROTON` but at first launch, instead of running under Wine, it tries to spin up a KVM guest and run there to escape the Wine sandbox floor.

**Mitigation:** the manifest's declared `EcosystemRuntime` is sealed by the publisher signature at admit time (S11.1 §5). The `EcosystemRuntime` field on the composed `SandboxProfile` (S3.2 §20) MUST equal the manifest's declaration; mismatch fails composition with `EcosystemRuntimeMismatch` and emits `APP_ECOSYSTEM_RUNTIME_BREAKOUT_ATTEMPTED` (per S3.2 §20.3). At Step F, the orchestrator invokes only the EcosystemRuntime adapter named by the manifest's runtime; an app that tries to spawn a different adapter (e.g., a Wine-declared app trying to invoke the KVM adapter) is denied at the S10.1 dispatch layer because the action's `target.adapter_id` is fixed at install time and the dispatch routing table does not have a path from `app.launch{ecosystem_runtime = RUNTIME_WINDOWS_PROTON}` into `runtime.kvm.boot_vm`. `ORCHESTRATION_KIND_MISMATCH_REJECTED` (FOREVER) is emitted.

### 12.5 AI-initiated launch of a quarantined app

**Adversary:** an AI subject with a bound `app.launch` capability tries to launch an app that has been quarantined by S12.1's capability-lie audit.

**Mitigation:** S2.3 hard-deny `AppQuarantineActive` rejects the action at policy evaluation, before Step A. The orchestrator never sees the action. `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` semantics (per S12.1) apply analogously for launch attempts; the quarantine state on the package binds at every action targeting the package. The AI subject can propose a launch action but cannot transition it through `policy_pending → executing` while the quarantine is active.

### 12.6 Sandbox profile substitution between Step C and Step E

**Adversary:** an attacker with file-system write access to the orchestrator's working directory swaps the composed `SandboxProfile` bytes after composition but before application, in an attempt to relax the floor before enforcement loads.

**Mitigation:** the `SandboxProfile.profile_id = prof_<hex>` is content-addressed (S3.2 §5.7). The orchestrator passes the `profile_id` to `ApplyProfile` and S3.2 re-validates that the loaded profile's content hash matches. A mismatch fails closed with `HOST_CAPABILITY_LIE` (S3.2 §6.3 — extended in spirit to profile tampering) and emits `TAMPER_DETECTED` (S3.1).

## 13. Worked examples

### 13.1 Steam game via Proton (`OrchestrationKind = WINE_PREFIX_NEW`)

```text
Setup:
  app_id = app:steam:hogwarts-legacy:1.0.5
  manifest.ecosystem_runtime = RUNTIME_WINDOWS_PROTON
  manifest.honesty_class = PARTIALLY_SUPPORTED
  manifest.wine_prefix_kind = PER_APP_FRESH
  manifest.gpu_class = GPU_FULL_3D
  manifest.network.allow = [*.steamcontent.com, *.steampowered.com]
  manifest.verification_probe = wayland_surface_visible(timeout=10s)

Step A: orchestrator reads manifest; declared EcosystemRuntime = RUNTIME_WINDOWS_PROTON.

Step B: composition inputs assembled.
  adapter_default = adapter:proton:9.0.0  (Wine/Proton runtime adapter)
  app_manifest    = manifest's required SandboxProfile
  user_request    = empty (no operator hint)
  policy_required = group g_default's policy bundle (allows GPU_FULL_3D in this group)
  runtime_safety_floor = ai_initiated_floor (subject is human, but this example
                         shows the orchestrator using the human floor; AI-initiated
                         flow uses ai_initiated_floor)

Step C: ComposeProfile returns profile_id = prof_a9b2... .
  ecosystem_runtime field on the profile = RUNTIME_WINDOWS_PROTON  (matches manifest;
  S3.2 §20 binds.)

Step D: OrchestrationKind selection.
  EcosystemRuntime = RUNTIME_WINDOWS_PROTON
  WinePrefixKind = PER_APP_FRESH (manifest)
  → OrchestrationKind = WINE_PREFIX_NEW

Step E: ApplyProfile applies prof_a9b2...; SANDBOX_PROFILE_APPLIED emitted.
  Filesystem floor:
    block_host_home = true
    deny includes /home/<u> and /aios/groups/<other>/* canonical paths
  GpuPolicy:
    bound to the group g_default's VkDevice (S8.2 per-group partition)
    GPU_FULL_3D approved for this composition

Step F: orchestrator invokes adapter:proton:9.0.0 with prof_a9b2... .
  Proton adapter creates a fresh prefix at:
    /aios/groups/g_default/users/u_op/runtime/wine/hogwarts-legacy/prefix
  dosdevices/z:: → /aios/groups/g_default/users/u_op/runtime/wine/hogwarts-legacy/zdrive
  (NOT to host home; INV-011 binds; S3.2 §9.1 floor enforces.)
  Proton starts the EXE; pid = 41520.
  Elapsed: 4.2 s (under WINE_PREFIX_NEW cold budget of 5 s).

Step G: verification probe = wayland_surface_visible(pid=41520, timeout=10s).
  Probe receives the game's first Wayland surface at t+7.8 s.
  Probe passes.

Step H: outcome = LAUNCHED.
  Records emitted:
    APP_LAUNCH_STARTED      (STANDARD_24M)
    WINE_PREFIX_CREATED     (STANDARD_24M; wine_prefix_kind = PER_APP_FRESH)
    APP_LAUNCH_SUCCEEDED    (STANDARD_24M; orchestration_kind = WINE_PREFIX_NEW)
  Metrics:
    app_launch_total{outcome="LAUNCHED",orchestration_kind="WINE_PREFIX_NEW"} ++
    app_wine_prefix_created_total{wine_prefix_kind="PER_APP_FRESH"} ++
    app_launch_duration_seconds{orchestration_kind="WINE_PREFIX_NEW"} observe(4.2s)
```

### 13.2 Android app via Waydroid (`OrchestrationKind = WAYDROID_CONTAINER_NEW`)

```text
Setup:
  app_id = app:apk:colornotes:2.4.7
  manifest.ecosystem_runtime = RUNTIME_ANDROID_WAYDROID
  manifest.honesty_class = PARTIALLY_SUPPORTED
  manifest.waydroid_isolation_level = PER_APP
  manifest.gpu_class = GPU_DISPLAY_ONLY
  manifest.network = DENY_ALL  (no GMS, no online sync available)
  manifest.verification_probe = wayland_surface_visible(timeout=20s)

Step A: orchestrator reads manifest; declared EcosystemRuntime = RUNTIME_ANDROID_WAYDROID.

Step B: composition inputs assembled.
  adapter_default = adapter:waydroid:1.4.0
  app_manifest    = manifest's required SandboxProfile (network DENY_ALL)
  user_request    = empty
  policy_required = group g_default's policy bundle
  runtime_safety_floor = human_initiated_floor

Step C: ComposeProfile returns profile_id = prof_e7c1... .
  ecosystem_runtime = RUNTIME_ANDROID_WAYDROID

Step D: OrchestrationKind selection.
  EcosystemRuntime = RUNTIME_ANDROID_WAYDROID
  No prior Waydroid container for this app
  → OrchestrationKind = WAYDROID_CONTAINER_NEW

Step E: ApplyProfile applies prof_e7c1... .

Step F: orchestrator invokes adapter:waydroid:1.4.0.
  Waydroid adapter creates a per-app LXC container at:
    /aios/groups/g_default/users/u_op/runtime/waydroid/colornotes/data
  Container's Android filesystem view sees only its own data;
  Android clipboard bridge installed; AIOS file portal bridge installed;
  no shared display memory with other Waydroid containers.
  APK launched; pid = 51022.
  Elapsed: 8.4 s (under WAYDROID_CONTAINER_NEW cold budget of 10 s).

Step G: verification probe = wayland_surface_visible(pid=51022, timeout=20s).
  App's first surface visible at t+11.1 s. Probe passes.

Step H: outcome = LAUNCHED.
  Records emitted:
    APP_LAUNCH_STARTED              (STANDARD_24M)
    WAYDROID_CONTAINER_STARTED      (STANDARD_24M; waydroid_isolation_level=PER_APP)
    APP_LAUNCH_SUCCEEDED            (STANDARD_24M)
```

### 13.3 Anti-cheat game via KVM (`OrchestrationKind = KVM_VM_BOOT`)

```text
Setup:
  app_id = app:steam:competitive-shooter:5.2.1
  manifest.ecosystem_runtime = RUNTIME_WINDOWS_VM
  manifest.honesty_class = REQUIRES_VM
  manifest.vm_fallback_kind = WINDOWS_ANTI_CHEAT
  manifest.vm.memory_mib = 16384
  manifest.vm.vcpus = 6
  manifest.vm.persistent = true  (save game state required)
  manifest.gpu_class = GPU_FULL_3D_PASSTHROUGH  (VFIO; gated)
  manifest.network.allow = [*.gameco-cdn.net]
  manifest.verification_probe = manifest_health_endpoint(
                                  guest_agent: "/aios/probe/game",
                                  timeout=30s)

  At install time, the operator approved:
    - REQUIRES_VM honesty class
    - 16 GiB RAM allocation
    - VFIO GPU passthrough binding to the secondary discrete GPU
    - vm_fallback_kind = WINDOWS_ANTI_CHEAT (the anti-cheat refuses Wine)

Step A: orchestrator reads manifest; declared EcosystemRuntime = RUNTIME_WINDOWS_VM.

Step B: composition inputs assembled.
  adapter_default = adapter:kvm:windows:2.1.0

Step C: ComposeProfile returns profile_id = prof_44de... .
  ecosystem_runtime = RUNTIME_WINDOWS_VM
  GpuPolicy = VFIO bound to the per-group VkDevice for g_default's
              secondary GPU; GPU_FULL_3D_PASSTHROUGH approved.

Step D: OrchestrationKind selection.
  EcosystemRuntime = RUNTIME_WINDOWS_VM
  → OrchestrationKind = KVM_VM_BOOT
  vm_fallback_kind = WINDOWS_ANTI_CHEAT (recorded in evidence)

Step E: ApplyProfile applies prof_44de... ; the qemu-system process is the
        sandbox target; storage_shares only includes the persistent disk
        path /aios/groups/g_default/users/u_op/runtime/vm/competitive-shooter/disk;
        evidence_bridge = true.

Step F: orchestrator invokes adapter:kvm:windows:2.1.0.
  KVM adapter unpauses (or boots cold) the persistent Windows guest with
  16 GiB RAM, 6 vCPU, VFIO-GPU. Guest agent handshake at t+38 s.
  Anti-cheat sees a real Windows kernel; passes its attestation. Game
  launches. pid (host-side qemu-system) = 8211; pid (guest-side game)
  is opaque to the host.
  Elapsed: 47 s (under KVM_VM_BOOT cold budget of 60 s).

Step G: verification probe = manifest_health_endpoint via the guest agent
        at "/aios/probe/game". Probe receives a 200-OK at t+58 s.
        Probe passes.

Step H: outcome = LAUNCHED.
  Records emitted:
    APP_LAUNCH_STARTED              (STANDARD_24M)
    KVM_VM_BOOTED                   (STANDARD_24M; vm_fallback_kind=WINDOWS_ANTI_CHEAT,
                                                    ephemeral=false)
    APP_LAUNCH_SUCCEEDED            (STANDARD_24M; orchestration_kind=KVM_VM_BOOT)
  Metrics:
    app_launch_total{outcome="LAUNCHED",orchestration_kind="KVM_VM_BOOT"} ++
    app_kvm_vm_booted_total{vm_fallback_kind="WINDOWS_ANTI_CHEAT",ephemeral="false"} ++
    app_launch_duration_seconds{orchestration_kind="KVM_VM_BOOT"} observe(47s)
  Operator-visible: "Game running in Windows VM. Anti-cheat compatible.
  Launched in 47 seconds."

When the operator quits the game and shuts down the VM:
    KVM_VM_TERMINATED               (STANDARD_24M; reason=OPERATOR_REQUEST)
```

## 14. Boundaries (what this contract does NOT do)

This contract is deliberately narrow. Things outside its scope:

- **It does not redefine `EcosystemRuntime`.** That enum lives in S12.1 §3.1; this contract consumes it.
- **It does not redefine `SandboxProfile`, `CompatibilityKind`, or the 5+1 composition algorithm.** Those live in S3.2; this contract calls `ComposeProfile` and `ApplyProfile`.
- **It does not own the `app.launch` action envelope shape.** That lives in S0.1 with the per-action target schema in the L6 app adapter manifest. This contract is what the `app.launch` adapter does internally.
- **It does not own the install path.** The first-time admit pipeline (Phase A → Phase B → S5.3 approval → S11.1 INSTALLING → ACTIVE → Phase C audit) lives in S12.1; this contract starts only after the app has reached `ACTIVE`.
- **It does not own the EcosystemRuntime adapters themselves.** Each adapter (Proton, Waydroid, KVM, Flatpak, AppImage, etc.) is its own `PackageKind = ADAPTER` package per S11.1 §3.4 and is shipped, signed, and updated independently.
- **It does not own the GPU device partition.** S8.2 owns per-group `VkDevice` discipline; this contract relies on that floor and never alters it.
- **It does not own the verification primitive catalog.** S2.4 owns the closed catalog of probes; this contract names probes but does not define them.
- **It does not own evidence retention or compaction.** S3.1 owns the evidence log; this contract queues record types for S3.1 Wave 8 consolidation.

## 15. Status and evidence

Status: `REAL` (initial; written 2026-05-09).

Evidence: `E1` — this file exists at the canonical path
`/aios/spec/L6/03_compatibility_runtime.md` and contains the structural contract listed above:

- closed enums declared (`OrchestrationKind` 8, `LaunchOutcome` 7, `WinePrefixKind` 3, `WaydroidIsolationLevel` 3, `VMFallbackKind` 4 → five closed enums total);
- proto IDL for the orchestration service (§4) with closed-typed external imports cited and never redefined;
- eight-step orchestration flow (§5) with explicit success/failure exits per step;
- per-`OrchestrationKind` launch budgets (§6);
- Wine, Waydroid, and KVM isolation discipline (§§7–9) with INV-011 and INV-017 bindings;
- bounded-cardinality telemetry contract (§10);
- ten evidence record types queued for S3.1 Wave 8 (§11);
- six adversary scenarios with mechanical mitigations (§12);
- three worked examples (§13);
- explicit boundaries (§14).

Promotion to higher evidence grades requires:

- **E2** — orchestrator process compiles against the schema package `aios.compatruntime.v1alpha1` and the imported S3.2 / S12.1 / S0.1 types. Blocked on initial implementation work in `crates/aios-compatruntime/`.
- **E3** — unit and integration tests exercising each `OrchestrationKind`, each `LaunchOutcome`, and each adversarial scenario in §12. Blocked on the EcosystemRuntime adapter packages reaching CONTRACT or higher.
- **E4** — end-to-end recovery rehearsal demonstrating that orchestrator failure during Step F or Step G correctly cleans up partially-started runtimes (Wine prefix half-created, Waydroid container started but verification probe pending, KVM guest booted but agent not yet ready). Blocked on E3.
- **E5** — live operational evidence on a fielded host running the three worked-example apps for ≥ 30 days without `WINE_PREFIX_BREAKOUT_ATTEMPTED`, `WAYDROID_ESCAPE_ATTEMPTED`, or `ORCHESTRATION_KIND_MISMATCH_REJECTED`.

INV-014 (no proof, no completion) binds the promotion path: `REAL` at E1 is permitted because the structural contract is complete; promotion past E1 requires the listed evidence and cannot be claimed without it.

## See also

- [S12.1 — App Runtime Model + Cross-Ecosystem Compatibility](01_app_runtime_model.md)
- [S3.2 — Sandbox Composition Language](04_sandbox_composition.md)
- [S11.1 — Repository Model](../L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [S8.2 — GPU Resource Model](../L8_Network_Hardware_Devices/05_gpu_resource_model.md)
- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [L0.4 — Constitutional Invariants (INV-002, INV-008, INV-011, INV-013, INV-017)](../L0_Governance_Evidence_Safety/04_invariants.md)
- [L6 Overview](00_overview.md)
- [Rev.1 §17 — Application, Package, and Compatibility Model](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
