# App Runtime Model + Cross-Ecosystem Compatibility (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists, structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Phase tag      | S12.1                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Layer          | L6 Apps, Packages, Compatibility                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Schema package | `aios.appcompat.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Consumes       | L0 INV-002 (AI proposes never executes), INV-008 (default-deny policy), INV-011 (cross-group access forbidden), INV-013 (AI cannot perform system admin), INV-017 (sandbox floor constitutional); S3.2 Sandbox Composition (`SandboxProfile`, `CompatibilityKind`, 5+1 source merge, runtime safety floor); S10.1 Capability Runtime gRPC (typed actions, `ActionDispatchKind = ISOLATED_SANDBOX`, lifecycle FSM); S11.1 Repository Model (`PackageKind = ADAPTER`, `RepositoryKind = AIOS_COMMUNITY_REPO`, `PublisherTrustLevel`, capability-lie audit); S5.3 Approval Mechanics (`request_approval`, `EXACT_ACTION` binding); S8.1 Network Policy (`NetworkOutboundManifest`); S8.2 GPU Resource Model (per-group VkDevice, `GpuPolicy`); S3.1 Evidence Log (`RecordType` vocabulary, `FOREVER`/`STANDARD_24M`/`EXTENDED_60M` retention classes); S4.1 Namespace Layout (per-group agent runtime paths) |
| Produces       | typed `EcosystemRuntime` / `EcosystemHonestyClass` / `ManifestTranslationStrategy` / `ObservedBehavior` / `RecipeTrustClass` / `ManifestDeltaOutcome` enums; the four-phase AI-assisted setup mechanism (Phase A observe → Phase B propose → Phase C audit → Phase D refine); the per-runtime contract for twelve closed ecosystem runtimes; the Community Recipe Registry contract over `AIOS_COMMUNITY_REPO`; the constitutional honesty principle (queued L0 candidate `ECOSYSTEM_HONESTY_DISCLOSURE`); fourteen evidence record types queued for S3.1 Wave 7; one `SandboxProfile.ecosystem_runtime` field queued for S3.2 Wave 7 consolidation; bounded-cardinality telemetry contract                                                                                                                                                                                                               |

## 1. Purpose

AIOS ships above the Linux kernel as a unified cognitive shell, and the operator does not care which ecosystem an application originated in. The operator wants to install a Steam game, an Android note-taker, a macOS CLI, a Linux-native IDE, and (sometimes) something that was only ever shipped for iPhone. The operator wants every one of those installs to be a typed action, gated by policy, sandboxed identically, audited identically, recoverable identically.

This sub-spec is the **app runtime model** that makes that uniformity real, and the **cross-ecosystem compatibility contract** that says, honestly and mechanically, what AIOS can and cannot do for foreign-ecosystem apps. Nothing in this contract reinvents distribution (S11.1 owns trust roots and admission), composition (S3.2 owns the sandbox), policy (S2.3 owns the decision), or execution dispatch (S10.1 owns the typed action runtime). What this contract adds is the layer **above** S3.2 that:

1. classifies foreign-app runtimes into a closed enum (`EcosystemRuntime`), each itself an AIOS package of `PackageKind = ADAPTER` (S11.1 §3.4);
2. binds each runtime to its honest scope (`EcosystemHonestyClass`) so the operator is told the truth — including "this cannot run on your hardware" when that is the truth;
3. defines the AI-assisted setup mechanism that turns an unknown foreign-app artifact (an APK, an AppImage, a Win32 EXE, a macOS bundle) into a signed AIOS app proposal that flows through S5.3 approval and S3.2 composition;
4. binds a Community Recipe Registry to S11.1 `AIOS_COMMUNITY_REPO` so successful operator-approved manifests can be contributed back, queried by other operators, and reputation-tracked — without ever bypassing local first-run audit.

The contract treats every foreign-app run as a typed AIOS action. There is no "shell out to wine64", no "just run waydroid", no untyped subprocess that escapes the sandbox floor. The cost of uniformity is that AIOS does not silently extend its reach to ecosystems it cannot honestly serve. iOS is the canonical example: Apple actively prevents iOS-binary execution on non-Apple hardware via secure-enclave-bound entitlements and hardware attestation. AIOS does not pretend otherwise. It returns the honest answer ("I cannot run iOS apps on this hardware; here is the closest practical alternative") and a typed remote bridge to the operator's actual Apple device when one exists. The honest answer is more trustworthy than a dishonest emulation that would fail when the operator discovers the limit.

## 2. Position in the system

```text
                  ┌──────────────────────────────────────────────────────────────┐
                  │                          OPERATOR                            │
                  │              (HUMAN_USER subject; per L4 identity)           │
                  └──────────────────────────────────────────────────────────────┘
                                              │
                                              │ install request
                                              ▼
                  ┌──────────────────────────────────────────────────────────────┐
                  │                       APP CATALOG                            │
                  │           (L7 marketplace surface — S11.1 admit pipeline)    │
                  └──────────────────────────────────────────────────────────────┘
                                              │
                                              │ recipe lookup
                                              ▼
                  ┌──────────────────────────────────────────────────────────────┐
                  │            COMMUNITY RECIPE REGISTRY (S11.1                  │
                  │            AIOS_COMMUNITY_REPO; signed; reputation-tracked)  │
                  └──────────────────────────────────────────────────────────────┘
                            │                                 │
                            │ recipe found                    │ recipe not found
                            │ (skip Phase A?  — never)        │
                            ▼                                 ▼
                  ┌─────────────────────────────────────────────────────────────┐
                  │   PHASE A — pre-flight observation (typed action            │
                  │   `app.observe_in_sandbox`; ISOLATED_SANDBOX; max-restricted) │
                  │   subject = `_system:service:app-observer`                  │
                  └─────────────────────────────────────────────────────────────┘
                                              │
                                              │ ObservedBehavior summary
                                              ▼
                  ┌─────────────────────────────────────────────────────────────┐
                  │   PHASE B — manifest proposal (typed action                 │
                  │   `app.translate_manifest`; AI fills SandboxProfile +       │
                  │   NetworkOutboundManifest + capability list +               │
                  │   EcosystemHonestyClass disclosure;                         │
                  │   AI never installs — INV-002)                              │
                  └─────────────────────────────────────────────────────────────┘
                                              │
                                              │ signed proposal
                                              ▼
                  ┌─────────────────────────────────────────────────────────────┐
                  │   S5.3 APPROVAL (HUMAN_USER approver; EXACT_ACTION binding) │
                  └─────────────────────────────────────────────────────────────┘
                                              │
                                              │ approval granted
                                              ▼
                  ┌─────────────────────────────────────────────────────────────┐
                  │   S11.1 INSTALL PIPELINE (admit → INSTALLING → ACTIVE)      │
                  └─────────────────────────────────────────────────────────────┘
                                              │
                                              ▼
                  ┌─────────────────────────────────────────────────────────────┐
                  │   PHASE C — first-run capability-lie audit (S11.1 §G;       │
                  │   60 s observation; quarantine on drift)                    │
                  └─────────────────────────────────────────────────────────────┘
                                              │
                                              ▼
                  ┌─────────────────────────────────────────────────────────────┐
                  │   APP RUNS under chosen EcosystemRuntime + composed         │
                  │   SandboxProfile (S3.2) + GpuPolicy (S8.2) + per-group      │
                  │   namespace (S4.1)                                          │
                  └─────────────────────────────────────────────────────────────┘
                                              │
                          ┌───────────────────┼────────────────────┐
                          │                   │                    │
                          ▼                   ▼                    ▼
                  ┌──────────────┐   ┌───────────────┐   ┌──────────────────┐
                  │ runtime ok   │   │ runtime breaks│   │ runtime breakout │
                  │ (steady)     │   │ (Phase D      │   │ attempted        │
                  │              │   │ delta loop)   │   │ (FOREVER ev.)    │
                  └──────────────┘   └───────────────┘   └──────────────────┘
```

This contract sits **above** S3.2 (S3.2 composes the sandbox; S12.1 selects the ecosystem runtime that feeds into it) and **below** L7 marketplace (the operator-facing UX is L7's job). It binds horizontally to S11.1 (the runtimes are themselves packages, the recipes are themselves community-repo objects, the capability-lie audit is shared) and to S10.1 (Phase A and Phase B are typed actions in the existing FSM).

## 3. Vocabulary (closed enums)

All vocabularies in this section are closed. Adding a value is a versioned spec change. Manifest validators, the install pipeline, and the Community Recipe Registry MUST reject values outside the enum at parse time. None of these enums admits an `OPEN` or `OTHER` value; the intent is to make ecosystem semantics fully mechanical and to make honest disclosure mechanical too.

### 3.1 `EcosystemRuntime`

Closed enum, twelve runtimes. Each runtime is itself an AIOS package of `PackageKind = ADAPTER` (S11.1 §3.4) with its own signed manifest, its own SandboxProfile floor declared in the adapter manifest, and its own `NetworkOutboundManifest` declaring what the runtime infrastructure may reach. An app that targets a given `EcosystemRuntime` consumes that runtime's defaults plus its own per-app additions through the standard S3.2 composition algorithm.

| Value                         | Source                      | Honest scope                                                                                                 | EcosystemHonestyClass        |
| ----------------------------- | --------------------------- | ------------------------------------------------------------------------------------------------------------ | ---------------------------- |
| `RUNTIME_LINUX_NATIVE`        | direct ELF execution        | Native ELF apps from distro repos; full first-class support.                                                 | `FULLY_SUPPORTED`            |
| `RUNTIME_FLATPAK`             | Flatpak rebuild             | Sandboxed Linux apps; capabilities translated 1:1 from Flatpak manifest.json finishes section.               | `FULLY_SUPPORTED`            |
| `RUNTIME_APPIMAGE`            | extract + run               | Portable Linux apps; AI extracts capabilities via Phase A pre-flight observation (no embedded manifest).     | `PARTIALLY_SUPPORTED`        |
| `RUNTIME_SNAP`                | snap rebuild                | Canonical-trust path; capabilities from snapcraft.yaml plugs/slots.                                          | `FULLY_SUPPORTED`            |
| `RUNTIME_DISTROBOX`           | container                   | Full Linux distro environment as one AIOS app; broad declared capabilities mandatory.                        | `PARTIALLY_SUPPORTED`        |
| `RUNTIME_WINDOWS_PROTON`      | Wine/Proton per-app prefix  | Win32 syscall translation; widely effective for Windows apps and games.                                      | `PARTIALLY_SUPPORTED`        |
| `RUNTIME_WINDOWS_VM`          | KVM + QEMU + Windows guest  | Full Windows VM for apps Wine cannot run (kernel-level anti-cheat, hard DRM, drivers).                       | `REQUIRES_VM`                |
| `RUNTIME_ANDROID_WAYDROID`    | Waydroid LXC + AOSP image   | Android apps without Google services; per-app data isolation; brokered clipboard.                            | `PARTIALLY_SUPPORTED`        |
| `RUNTIME_ANDROID_VM_WITH_GMS` | KVM + AOSP + GMS image      | Android apps that require Play Services; Play Integrity remains best-effort.                                 | `REQUIRES_VM`                |
| `RUNTIME_MACOS_DARLING`       | Darling translation         | Subset of macOS CLI apps; limited GUI support.                                                               | `PARTIALLY_SUPPORTED`        |
| `RUNTIME_MACOS_VM`            | KVM (OSX-KVM) + macOS guest | macOS apps with hardware emulation; legal grey area documented in adapter manifest.                          | `REQUIRES_VM`                |
| `RUNTIME_REMOTE_APPLE_BRIDGE` | VNC/RDP-equivalent bridge   | Bridge to the operator's actual Apple device for iOS apps; AIOS does not lie about iOS execution capability. | `NOT_RUNNABLE_ON_NON_NATIVE` |

The enum is closed. Values not in this list are rejected by:

- the install pipeline (S11.1 §5) at manifest validation;
- the Community Recipe Registry at recipe ingest (§7);
- the Phase B `app.translate_manifest` proposer (any AI-emitted proposal naming an unknown runtime is rejected).

### 3.2 `EcosystemHonestyClass`

Closed enum, four classes. The class is constitutionally required disclosure on every package install. The L7 marketplace surface MUST display it. The operator approval prompt (S5.3) MUST include it.

| Value                        | Semantics                                                                                                                                                                                            |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `FULLY_SUPPORTED`            | Runs as a first-class AIOS app. No translation surprise. Capability declaration is direct. Performance and feature parity are at native level subject to host hardware.                              |
| `PARTIALLY_SUPPORTED`        | Works for many apps within the runtime; specific compatibility issues are disclosed per app (e.g., "this Android app falls back to local-only because Google services are not available").           |
| `REQUIRES_VM`                | Needs a full VM runtime; performance, RAM, disk, and boot-time implications are disclosed. Approval prompt explicitly lists VM resource requirements.                                                |
| `NOT_RUNNABLE_ON_NON_NATIVE` | Explicit "this cannot run on non-Apple/whatever hardware; here is the closest practical alternative." The marketplace surface presents alternatives (e.g., Linux-native build; remote Apple bridge). |

The honesty principle (§8) is constitutional in spirit and is queued as candidate L0 invariant `ECOSYSTEM_HONESTY_DISCLOSURE` for the next L0 revision (narrative-only mention here; the L0 entry is authored separately).

### 3.3 `ManifestTranslationStrategy`

Closed enum, eight strategies. The strategy is the input to the Phase B `app.translate_manifest` typed action and determines how the AI proposer derives `SandboxProfile + NetworkOutboundManifest + capability list` from the foreign artifact.

| Value                   | Source manifest                      | Translation rule                                                                                                                               |
| ----------------------- | ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `ANDROID_MANIFEST_XML`  | `AndroidManifest.xml`                | Parse `<uses-permission>` entries and map to AIOS capabilities via the closed Android-permission-to-AIOS-capability mapping (versioned table). |
| `FLATPAK_MANIFEST_JSON` | Flatpak `manifest.json`              | Parse the `finishes` section; direct 1:1 mapping to AIOS capabilities and `NetworkOutboundManifest` allow rules.                               |
| `SNAPCRAFT_YAML`        | `snapcraft.yaml`                     | Parse plugs/slots; direct mapping.                                                                                                             |
| `PROTON_RECIPE`         | ProtonDB + WineHQ AppDB              | Pull rating + recipe metadata (with attribution); combine with WineHQ AppDB notes; emit AIOS recipe.                                           |
| `WINE_PREFIX_PROBE`     | (no source manifest; Win32 EXE only) | Phase A observation in a max-restricted Wine prefix; AI extracts capabilities from observed syscalls.                                          |
| `APPIMAGE_BEHAVIORAL`   | (no embedded manifest)               | Phase A behavioral extraction; AI proposes capabilities from observation.                                                                      |
| `MAC_INFO_PLIST`        | macOS `Info.plist` Entitlements      | Parse Entitlements + UsageDescription strings; map to AIOS capabilities via the closed macOS-entitlement-to-AIOS-capability mapping.           |
| `IOS_REMOTE_BRIDGE`     | (no translation; bridge only)        | No local translation. Recipe configures the remote bridge target (operator's actual iOS device); AIOS does not pretend to run iOS locally.     |

The strategy is selected automatically by the Phase B proposer based on the artifact format. The selection is recorded in the manifest proposal and audited by S11.1 admission. A recipe whose strategy is `IOS_REMOTE_BRIDGE` cannot legally claim `EcosystemRuntime` other than `RUNTIME_REMOTE_APPLE_BRIDGE`; mismatch is rejected at registry ingest.

### 3.4 `ObservedBehavior` summary fields

Phase A produces a typed summary of what the artifact did under max-restricted observation. The summary is signed by the observer subject and attached to the Phase B proposal.

```proto
message ObservedBehavior {
  string observation_id = 1;                 // obs_<ulid26>
  string artifact_hash = 2;                  // hex_lower(BLAKE3(content))[:32]
  google.protobuf.Duration observed_for = 3; // ≤ 300 s
  repeated SyscallClass observed_syscalls = 10;
  repeated string blocked_filesystem_reads = 11;   // canonical paths
  repeated string blocked_filesystem_writes = 12;
  repeated string attempted_dns_resolutions = 13;  // FQDNs only; never raw IP
  repeated string attempted_outbound_endpoints = 14; // canonicalised endpoints
  bool attempted_gpu_init = 15;
  bool attempted_audio_init = 16;
  bool attempted_microphone_open = 17;
  bool attempted_camera_open = 18;
  bool attempted_clipboard_read = 19;
  bool attempted_clipboard_write = 20;
  repeated string error_messages_redacted = 30;     // PII-stripped
  bool process_terminated_normally = 31;
  uint32 exit_code = 32;
}

enum SyscallClass {
  SYSCALL_CLASS_UNSPECIFIED = 0;
  FILESYSTEM_READ = 1;
  FILESYSTEM_WRITE = 2;
  NETWORK_OUTBOUND = 3;
  NETWORK_INBOUND = 4;
  PROCESS_FORK = 5;
  PROCESS_EXEC = 6;
  IPC = 7;
  GPU_SUBMIT = 8;
  AUDIO = 9;
  CLIPBOARD = 10;
}
```

`SyscallClass` is a closed enum. The observation cannot record raw secret data, raw clipboard contents, raw filesystem bytes, or raw network payloads — only the structural fact that the action was attempted (INV-015 "evidence never contains secrets" applies recursively here).

### 3.5 `RecipeTrustClass`

Closed enum, four classes for the Community Recipe Registry. A recipe carries a trust class derived from its publisher tier (S11.1 §3.1 `PublisherTrustLevel`) and its recipe-specific reputation history.

| Value                 | Semantics                                                                                                                                      |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `RECIPE_AIOS_CURATED` | Authored or curated by `AIOS_ROOT` or `VERIFIED` publishers; signed by the publisher; AIOS-root-cosigned for headline apps.                    |
| `RECIPE_COMMUNITY`    | Authored by `COMMUNITY` publishers or contributed by individual operators; signed by contributor; subject to first-run audit on every install. |
| `RECIPE_IMPORTED`     | One-shot import from upstream (ProtonDB, Flathub, AUR, Snapcraft); attribution preserved; never implicit-trust; subject to first-run audit.    |
| `RECIPE_QUARANTINED`  | Recipe whose history shows capability-lie events; auto-quarantined; subject to operator review before further use.                             |

A recipe's trust class never grants policy authority by itself. The S5.3 approval prompt remains mandatory; the trust class only changes how the prompt is presented and how loud the disclosure is.

### 3.6 `ManifestDeltaOutcome`

Closed enum, four outcomes for the Phase D continuous-refinement flow.

| Value                  | Semantics                                                                                                                           |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `DELTA_PROPOSED`       | AI emitted a manifest delta proposal with evidence; awaiting operator approval.                                                     |
| `DELTA_APPROVED`       | Operator approved; new versioned manifest is signed and recorded; old manifest is retained for audit.                               |
| `DELTA_REJECTED`       | Operator rejected the proposed delta; existing manifest remains in force; reason recorded for future Phase D loops.                 |
| `DELTA_CAPABILITY_LIE` | Phase D observation showed the app was attempting an undeclared capability that was not the cause of the breakage; auto-quarantine. |

## 4. The four-phase mechanism

The four phases are the contract surface that turns an unknown foreign-app artifact into an AIOS app under the standard typed-action machinery. Each phase is a typed action; the AI is bound to **propose** at every step (INV-002), and the operator is the only subject that can transition the proposal into an installed runtime state.

### 4.1 Phase A — pre-flight observation

**Typed action:** `app.observe_in_sandbox`

**Dispatch:** `ActionDispatchKind = ISOLATED_SANDBOX` (per S10.1).

**Subject discipline:** the observer subject is `_system:service:app-observer` — the AI agent acts under a system-scope service identity, never under the operator's identity. Per INV-013, the observer cannot mutate `/aios/system/...` regardless of any capability binding. Per INV-002, the observer cannot install; it can only observe.

**Inputs:**

```text
- artifact_blob (APK / ELF / EXE / AppImage / DMG / IPA)
- artifact_hash = "obs_" + ulid26()  (observation id; not artifact id)
- max_observation_duration: Duration  (default 30 s, hard cap 300 s)
- ManifestTranslationStrategy hint    (auto-detected if absent)
```

**Sandbox profile floor** (declared in the `_system:service:app-observer` adapter's default; non-negotiable):

```yaml
filesystem:
  root_mode: NO_ACCESS
  allow_write:
    - /aios/system/runtime/observer/{observation_id}/scratch # ephemeral; tmpfs
  tmpfs_for_tmp: true
  home_isolation: true
network:
  mode: LOOPBACK_ONLY # no outbound; observed DNS attempts logged
  dns_brokered: true # broker captures attempted FQDNs
  block_metadata_endpoints: true
process:
  seccomp_profile_id: aios.observer-narrow
  no_new_privileges: true
  drop_all_capabilities: true
  allow_ptrace: false
  allow_user_namespace: false
  max_processes: 50
resources:
  cpu_weight: 100 # low priority
  memory_max_bytes: 2147483648 # 2 GiB hard cap
  pids_max: 200
secrets:
  mode: NO_SECRET_ACCESS
evidence:
  capture_stdout: REDACTED # observer captures structural facts only
  capture_stderr: REDACTED
gpu_policy:
  gpu_capability_class: GPU_NONE # Phase A cannot observe under GPU; that is Phase D after approval
  deny_compute_pipeline: true
  deny_validation_layers: true
```

**Outputs:** an `ObservedBehavior` record (§3.4) signed by the observer subject's runtime key, recorded as `APP_OBSERVE_COMPLETED` evidence (S3.1; §13).

**Time budget:** soft 30 s, hard 300 s. Apps that need extended observation (slow startup, license check, network handshake retry) carry an explicit `max_observation_duration` request that the operator can pre-approve via S5.3 in the install action envelope.

**Termination conditions:**

- artifact's main process exits → `process_terminated_normally = true`; observation closes;
- hard timeout reached → observation closes with `APP_OBSERVE_TIMEOUT` evidence (extended-60M retention); the partial `ObservedBehavior` is still produced and feeds Phase B;
- artifact attempts breakout (e.g., escape from the namespace, attempted ptrace of host) → observation aborts; `APP_ECOSYSTEM_RUNTIME_BREAKOUT_ATTEMPTED` evidence (FOREVER); artifact is added to a per-host blocklist for further observation attempts.

### 4.2 Phase B — manifest proposal

**Typed action:** `app.translate_manifest`

**Dispatch:** `ActionDispatchKind = ISOLATED_SANDBOX` (the proposer runs in its own sandbox; it does NOT install).

**Subject discipline:** the proposer is an AI subject (`is_ai = true`). Per INV-002, the proposer can emit a proposal and submit it for approval, but cannot transition the install action through `policy_pending → executing`.

**Inputs:**

```text
- ObservedBehavior from Phase A
- foreign-app source manifest (if any) — Info.plist, AndroidManifest.xml, snapcraft.yaml, manifest.json
- ManifestTranslationStrategy
- target EcosystemRuntime (must be consistent with strategy; cross-check)
- target install_scope (USER_SCOPED / GROUP_SCOPED / SYSTEM_ONLY)
```

**Outputs:** a signed proposal:

```proto
message AppManifestProposal {
  string proposal_id = 1;                       // prop_<ulid26>
  string app_id = 2;                            // app_<ulid26> assigned at proposal time
  EcosystemRuntime ecosystem_runtime = 3;
  EcosystemHonestyClass honesty_class = 4;
  ManifestTranslationStrategy strategy = 5;
  aios.sandbox.v1alpha1.SandboxProfile sandbox_profile = 6;
  aios.network.v1alpha1.NetworkOutboundManifest network_outbound_manifest = 7;
  repeated string declared_capabilities = 8;    // capability ids per S1.1
  string observed_behavior_hash = 9;            // BLAKE3 of ObservedBehavior
  string proposer_subject_canonical_id = 10;
  bytes proposer_signature = 11;                // Ed25519
  string honesty_disclosure_text = 12;          // operator-visible plain-language rationale
  repeated string compatibility_caveats = 13;   // e.g., "Anti-cheat refuses Wine; switch to RUNTIME_WINDOWS_VM if multiplayer is required"
}
```

**Approval requirement:** `HUMAN_USER` per S5.3. The approval prompt is bound to the proposal_id via `EXACT_ACTION` binding (S5.3) — re-using the approval for a different proposal is rejected. The prompt MUST display:

- the `EcosystemRuntime` and `EcosystemHonestyClass`;
- the `honesty_disclosure_text` and `compatibility_caveats`;
- the declared capability list (closed-enum names; never raw shell-equivalent prose);
- the `NetworkOutboundManifest` allow list as plain-language summary;
- the `SandboxProfile` summary (filesystem root mode, network mode, secrets mode, GPU class).

**INV-002 binding:** the AI subject NEVER calls install. Even if the proposer holds a bound capability covering install, the action runtime rejects the transition `policy_pending → executing` for an AI-submitted install action; `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` evidence (FOREVER) is emitted on every such attempt.

### 4.3 Phase C — first-run capability-lie audit

This phase is **already specified** in S11.1 §G as the 60-second first-run capability audit. This contract does NOT redefine it; it cites it and binds the additional ecosystem-runtime context.

**Cited from S11.1 §G:**

- 60 s observation window after first ACTIVE transition;
- declared capabilities (from approved manifest) compared against runtime-observed capabilities;
- any observed capability not declared → `QUARANTINED` + `CAPABILITY_LIE_DETECTED` evidence (FOREVER retention).

**Ecosystem-runtime additions specified here:**

- the audit attaches the `EcosystemRuntime` and `EcosystemHonestyClass` to the audit record;
- a `FULLY_SUPPORTED`-claimed app whose Phase C observation shows behavior consistent with `NOT_RUNNABLE_ON_NON_NATIVE` (e.g., the app immediately attempts secure-enclave hardware attestation on non-Apple hardware) emits `APP_HONESTY_CLASS_VIOLATION` evidence (FOREVER) in addition to the standard capability-lie path;
- continuous monitoring extends past the 60 s window: a sustained pattern of capability lies that begins after the audit window also triggers `QUARANTINED` (this addresses the "60 s is too short" adversary; see §10.1).

### 4.4 Phase D — continuous refinement

**Typed action:** `app.propose_manifest_delta`

**Dispatch:** `ActionDispatchKind = ISOLATED_SANDBOX`.

**Subject discipline:** AI subject (`is_ai = true`); INV-002 applies — proposal only, never direct apply.

**Trigger conditions:**

- the runtime breaks at runtime (e.g., a Windows app blocked DNS to its license server, then refuses to start);
- the operator reports "this app used to work and now does not" via the L7 admin surface;
- a Phase C extended-window monitor flags a recurring capability denial that prevents the app from operating;
- a recipe-side update (S11.1 STABLE channel) carries a new manifest version that the operator must explicitly accept.

**Mechanism:**

1. AI re-observes the failure in a restricted Phase-A-equivalent sandbox (different `observation_id`, same observer subject discipline).
2. AI extracts the specific deltas: blocked DNS to `server.gameco.com`, blocked filesystem read of `~/Documents/GameSaves`, etc.
3. AI cross-references publisher metadata and the original recipe to attach evidence: "this is the game's official endpoint per ProtonDB recipe 9281".
4. AI emits a `app.propose_manifest_delta` proposal with `ManifestDeltaOutcome = DELTA_PROPOSED`.
5. Operator reviews the delta (S5.3 approval prompt; `EXACT_ACTION` bound to the new manifest version hash).
6. On approval (`DELTA_APPROVED`): a new versioned manifest is signed and recorded; the old manifest is retained for audit and rollback; `APP_MANIFEST_DELTA_APPROVED` evidence (STANDARD_24M) is emitted.
7. On rejection (`DELTA_REJECTED`): the existing manifest remains in force; the rejection reason is recorded for future Phase D loops; `APP_TRANSLATE_MANIFEST_REJECTED` evidence (extended-60M) is emitted to keep the rejection audit-visible.
8. If the Phase D observation reveals an undeclared capability that is NOT the cause of the breakage: `DELTA_CAPABILITY_LIE` outcome; auto-quarantine; `APP_HONESTY_CLASS_VIOLATION` or `CAPABILITY_LIE_DETECTED` (whichever applies) FOREVER evidence.

Phase D never silently widens the manifest. Every widening is an explicit operator decision recorded in evidence.

## 5. Per-runtime contracts

Each `EcosystemRuntime` is itself a `PackageKind = ADAPTER` package (S11.1 §3.4). The adapter declares its default `SandboxProfile` floor through the standard S3.2 `AdapterDefault` source. The floor is the constitutional minimum for any app running under the runtime; per S3.2 §5.4, no upstream source can loosen it — the runtime safety floor still wins over the runtime-adapter default if they ever disagree.

The per-runtime sections below are short on purpose: they declare what the runtime consumes, what its declared SandboxProfile floor looks like, and which `EcosystemHonestyClass` it inherits. The full profile schema is S3.2's job.

### 5.1 `RUNTIME_LINUX_NATIVE`

- **Consumes:** native ELF binaries from S11.1 `AIOS_VERIFIED_REPO` or `AIOS_COMMUNITY_REPO`.
- **Floor:** `filesystem.root_mode = READ_ONLY`, `network.mode = EXPLICIT_ALLOWLIST`, `secrets.mode = BROKER_ONLY`, GPU class declared per app, no compatibility wrapper.
- **Honesty class:** `FULLY_SUPPORTED`.
- **Translation strategy:** none — Linux-native packages carry their own AIOS manifest at admit time.

### 5.2 `RUNTIME_FLATPAK`

- **Consumes:** Flatpak packages with `manifest.json`.
- **Floor:** Flatpak finishes section is translated 1:1; AIOS sandbox floor still applies; Flatpak's broader-than-AIOS finishes are tightened to AIOS levels.
- **Honesty class:** `FULLY_SUPPORTED` (Flatpak is structured and well-understood).
- **Translation strategy:** `FLATPAK_MANIFEST_JSON`.

### 5.3 `RUNTIME_APPIMAGE`

- **Consumes:** AppImage SquashFS bundles.
- **Floor:** stricter than Linux native because AppImages have no embedded manifest; per-app data lives under `/aios/groups/<group_id>/agents/<agent_id>/runtime/appimage/<app_id>/data` (per S3.2 §18.4); FUSE mount is brokered.
- **Honesty class:** `PARTIALLY_SUPPORTED` — some AppImages need extra Phase A observation rounds to extract their actual capability set.
- **Translation strategy:** `APPIMAGE_BEHAVIORAL`.

### 5.4 `RUNTIME_SNAP`

- **Consumes:** snap packages with `snapcraft.yaml`.
- **Floor:** plugs and slots map to AIOS capabilities directly; snap interfaces beyond AIOS coverage are denied; per-app data isolated.
- **Honesty class:** `FULLY_SUPPORTED`.
- **Translation strategy:** `SNAPCRAFT_YAML`.

### 5.5 `RUNTIME_DISTROBOX`

- **Consumes:** OCI base images turned into per-user Linux distro environments.
- **Floor:** the entire distrobox is one AIOS app with broad declared capabilities (filesystem read+write to `/aios/groups/.../runtime/distrobox/<container_id>/data`, network explicit-allowlist, no GPU compute by default); the operator is explicitly told the broader capability scope at install.
- **Honesty class:** `PARTIALLY_SUPPORTED` — the container hosts other binaries; their behaviour is opaque to AIOS without a sub-app sandbox; this is disclosed.
- **Translation strategy:** chosen per inner app — typically `WINE_PREFIX_PROBE` or `APPIMAGE_BEHAVIORAL` is reused for inner artifacts.

### 5.6 `RUNTIME_WINDOWS_PROTON`

- **Consumes:** Win32/PE binaries — installers, games, utilities; per-app prefix.
- **Floor:** S3.2 §9.1 Wine/Proton profile applies (private prefix, blocked host home, portal file picker, narrow seccomp); per-prefix path under `/aios/groups/<group_id>/agents/<agent_id>/runtime/wine/<app_id>` per S3.2 §18.4.
- **Honesty class:** `PARTIALLY_SUPPORTED` — many apps work; kernel-level anti-cheat refuses; the app's `compatibility_caveats` field surfaces this honestly to the operator.
- **Translation strategy:** `PROTON_RECIPE` (recipe found) or `WINE_PREFIX_PROBE` (no recipe).

### 5.7 `RUNTIME_WINDOWS_VM`

- **Consumes:** Win32 binaries that Wine cannot run (anti-cheat, hard DRM, kernel drivers).
- **Floor:** S3.2 §9.3 VM-fallback profile applies (explicit storage shares only, evidence bridge mandatory); VM disk under `/aios/groups/<group_id>/agents/<agent_id>/runtime/vm/<app_id>/disk.qcow2`.
- **Honesty class:** `REQUIRES_VM` — the operator is told the RAM/disk/boot-time costs explicitly at the approval prompt.
- **Translation strategy:** `PROTON_RECIPE` may seed network/capability hints; the VM image itself is curated by `AIOS_VERIFIED` publishers.

### 5.8 `RUNTIME_ANDROID_WAYDROID`

- **Consumes:** APKs that do not require Google services.
- **Floor:** S3.2 §9.2 Waydroid profile applies (per-app data, brokered clipboard, file portal only); the Waydroid container runs under the per-group AIOS namespace + S8.2 per-group VkDevice (so a Waydroid breakout does not grant cross-group access — INV-011).
- **Honesty class:** `PARTIALLY_SUPPORTED` — apps depending on Play Integrity refuse; surfaced via `compatibility_caveats`.
- **Translation strategy:** `ANDROID_MANIFEST_XML`.

### 5.9 `RUNTIME_ANDROID_VM_WITH_GMS`

- **Consumes:** APKs that require Play Services.
- **Floor:** S3.2 §9.3 VM profile applies; AOSP+GMS image curated by `AIOS_VERIFIED` publishers; Play Integrity remains best-effort and is disclosed honestly in the install prompt.
- **Honesty class:** `REQUIRES_VM`.
- **Translation strategy:** `ANDROID_MANIFEST_XML`.

### 5.10 `RUNTIME_MACOS_DARLING`

- **Consumes:** macOS CLI tools and a subset of GUI apps.
- **Floor:** Darling host is wrapped similarly to Wine; per-prefix path under `/aios/groups/.../runtime/darling/<app_id>`.
- **Honesty class:** `PARTIALLY_SUPPORTED` — GUI support is limited; surfaced honestly.
- **Translation strategy:** `MAC_INFO_PLIST`.

### 5.11 `RUNTIME_MACOS_VM`

- **Consumes:** full macOS apps requiring hardware emulation.
- **Floor:** S3.2 §9.3 VM profile + adapter manifest documents the legal grey area (Apple's macOS EULA on non-Apple hardware); the operator approval prompt MUST display the legal-grey-area note verbatim from the adapter manifest.
- **Honesty class:** `REQUIRES_VM`.
- **Translation strategy:** `MAC_INFO_PLIST`.

### 5.12 `RUNTIME_REMOTE_APPLE_BRIDGE`

- **Consumes:** iOS apps the operator wants to use; the bridge connects to the operator's actual iPhone or iPad.
- **Floor:** the bridge runtime declares `network.mode = EXPLICIT_ALLOWLIST` to the operator's device endpoint only; clipboard brokered; no local execution of iOS binaries — none.
- **Honesty class:** `NOT_RUNNABLE_ON_NON_NATIVE` — the operator approval prompt explicitly says: "AIOS cannot run iOS apps on this hardware. This bridge connects to your Apple device. The app runs on your device; AIOS shows you its surface."
- **Translation strategy:** `IOS_REMOTE_BRIDGE`.

A recipe declaring `RUNTIME_REMOTE_APPLE_BRIDGE` MUST set the strategy to `IOS_REMOTE_BRIDGE`. A recipe with strategy `IOS_REMOTE_BRIDGE` and any other runtime is rejected at registry ingest with `APP_RECIPE_DECEPTIVE_REJECTED_AT_INGEST` (FOREVER) — see §10.6.

## 6. The community recipe registry

The Community Recipe Registry is a layer **above** S11.1 `AIOS_COMMUNITY_REPO`. It does not replace that repo; it adds recipe-shaped objects that operators can contribute, search, and reuse. The registry is content-addressed, signed, and reputation-tracked.

### 6.1 Recipe shape

```proto
message AppRecipe {
  string recipe_id = 1;                            // recipe:<vendor>:<app_name>:<version>
  string canonical_id = 2;                         // hex_lower(BLAKE3(jcs(this_without_signature)))[:32]
  EcosystemRuntime ecosystem_runtime = 3;
  EcosystemHonestyClass honesty_class = 4;
  ManifestTranslationStrategy strategy = 5;
  aios.sandbox.v1alpha1.SandboxProfile sandbox_profile = 6;
  aios.network.v1alpha1.NetworkOutboundManifest network_outbound_manifest = 7;
  repeated string declared_capabilities = 8;
  string evidence_hash = 9;                        // BLAKE3 of attached evidence pack
  string contributor_subject_canonical_id = 10;
  bytes contributor_signature = 11;                 // Ed25519
  RecipeTrustClass trust_class = 12;
  RecipeReputation reputation = 13;
  google.protobuf.Timestamp first_published_at = 14;
  google.protobuf.Timestamp last_updated_at = 15;
  repeated string upstream_attribution = 16;        // e.g., ["protondb:9281", "winehq-appdb:Hogwarts-Legacy"]
}

message RecipeReputation {
  uint64 successful_install_count = 1;             // operators who installed and never reported a lie
  uint64 capability_lie_event_count = 2;           // operators whose Phase C audit fired
  uint64 manifest_delta_approved_count = 3;
  uint64 manifest_delta_rejected_count = 4;
  uint64 quarantine_event_count = 5;
}
```

The recipe id format is `recipe:<vendor>:<app_name>:<version>` (lowercase, dot-separated where ambiguous; e.g., `recipe:steam:hogwarts-legacy:1.0.5`). The canonical id is the content hash and is the primary lookup key.

### 6.2 Contribution

A successful app installation can contribute back via the typed action `app.contribute_recipe`:

```text
inputs:  app_id, manifest_version, evidence_pack
output:  AppRecipe signed by contributor; published to AIOS_COMMUNITY_REPO
```

Contribution is **never required**. It is opt-in per operator, per app, with an explicit S5.3 approval prompt. Operators can also contribute anonymously (a per-recipe ephemeral signing key derived from the operator's identity but not back-traceable on the registry surface; the original signing key remains in the operator's vault).

### 6.3 Reputation

Reputation evidence is structural:

- `successful_install_count` increments when an operator runs the recipe and Phase C passes;
- `capability_lie_event_count` increments when Phase C fires `CAPABILITY_LIE_DETECTED` against this recipe (FOREVER evidence cross-referenced);
- `manifest_delta_approved_count` increments per Phase D `DELTA_APPROVED`;
- `manifest_delta_rejected_count` increments per Phase D `DELTA_REJECTED`;
- `quarantine_event_count` increments when a recipe is quarantined.

The registry exposes reputation as part of the recipe lookup; the L7 marketplace surface displays it to the operator.

A recipe whose `capability_lie_event_count / successful_install_count > 0.05` is auto-promoted to `RECIPE_QUARANTINED`. A recipe in `RECIPE_QUARANTINED` is not surfaced to operators except via an explicit "show quarantined" search filter; further use requires an explicit operator override and FOREVER evidence.

### 6.4 Curation

`AIOS_VERIFIED` publishers can curate `RECIPE_AIOS_CURATED` recipes for popular apps. Curation is a normal package publish through S11.1 with `PackageKind = ADAPTER` carrying the recipe payload — no new admission path is created. Curated recipes are surfaced first in marketplace search.

### 6.5 Imported recipes

The registry supports one-shot translation actions to import upstream community knowledge:

| Source            | Translation                                                                                                                       | Resulting trust class |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------- | --------------------- |
| ProtonDB ratings  | Combined with WineHQ AppDB notes; Phase B `app.translate_manifest` strategy = `PROTON_RECIPE`; attribution preserved.             | `RECIPE_IMPORTED`     |
| Flathub manifests | Direct Flatpak manifest → AIOS recipe with `RUNTIME_FLATPAK`; attribution preserved.                                              | `RECIPE_IMPORTED`     |
| AUR PKGBUILDs     | AUR script → AIOS Linux-native recipe with `RUNTIME_LINUX_NATIVE`; capability declarations from PKGBUILD `depends`/`makedepends`. | `RECIPE_IMPORTED`     |
| Snapcraft store   | Snapcraft.yaml + store metadata → AIOS recipe with `RUNTIME_SNAP`; attribution preserved.                                         | `RECIPE_IMPORTED`     |

**Trust model:** imported recipes inherit upstream's reputation as **metadata only** — the imported reputation is never substituted for AIOS-local first-run audit. Phase A pre-flight observation **always** runs locally; Phase C first-run audit **always** runs locally. The import is metadata-only, not implicit-trust. **Local observation always wins.** A successfully imported recipe with high upstream reputation but a local capability-lie event is still quarantined.

Imported recipes always carry `upstream_attribution` so operators see the chain of provenance.

## 7. The honesty principle

This is the constitutional rule that defines what AIOS will and will not promise.

> AIOS does not promise capabilities it does not have. iOS apps cannot run on non-Apple hardware because Apple actively prevents it (secure enclave, entitlements, hardware attestation). AIOS does not pretend otherwise. The honest answer ("I cannot do this; here is the closest practical alternative") is more trustworthy than a dishonest emulation that fails when the user discovers the limit.

The honesty principle is encoded in this contract through three mechanical rules:

**Rule 1 — `EcosystemHonestyClass` is mandatory disclosure.** Every package install — regardless of trust class, regardless of curation level, regardless of whether the recipe is imported or community-contributed — MUST display the `EcosystemHonestyClass` to the operator at the S5.3 approval prompt.

**Rule 2 — `NOT_RUNNABLE_ON_NON_NATIVE` is the only honest answer for ecosystems AIOS cannot host.** When the operator searches for an iOS app, the marketplace surface shows the `RUNTIME_REMOTE_APPLE_BRIDGE` option with `NOT_RUNNABLE_ON_NON_NATIVE` disclosure plus the closest practical alternatives (Linux-native build if it exists; remote bridge if the operator has an Apple device). AIOS does not surface a fictional "iOS emulator" option.

**Rule 3 — honesty-class violations are FOREVER evidence.** A package that claims `FULLY_SUPPORTED` and Phase C/D observation reveals behavior consistent with `NOT_RUNNABLE_ON_NON_NATIVE` emits `APP_HONESTY_CLASS_VIOLATION` (FOREVER retention) and is auto-quarantined. The recipe's `capability_lie_event_count` and `quarantine_event_count` both increment.

**Constitutional candidate.** This contract queues `ECOSYSTEM_HONESTY_DISCLOSURE` as a candidate L0 invariant for the next L0 revision. The narrative-only intent: "AIOS shall not present an EcosystemHonestyClass weaker than the runtime is verified to deliver. Honesty class disclosure is mandatory at install and at every operator-visible surface." The L0 invariant authoring is **not in scope** for this contract.

## 8. Cross-spec dependencies and how this composes

The mechanism reuses existing AIOS machinery without duplication. The table makes the reuse explicit and notes which item is queued for a future Wave consolidation.

| Concept                                        | Where it lives                                    | Note                                                                                                                      |
| ---------------------------------------------- | ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| EcosystemRuntime as AIOS package               | S11.1 §3.4 `PackageKind = ADAPTER`                | already closed enum; no schema change needed in S11.1.                                                                    |
| Sandbox composition with ecosystem layer       | S3.2 §5 6-source merge                            | the `adapter_default` source is the EcosystemRuntime adapter's declared profile floor.                                    |
| App manifest translation                       | S10.1 typed action `app.translate_manifest`       | new typed action added to the S10.1 dispatch table; **queued** for S10.1 next-wave consolidation.                         |
| Pre-flight observation                         | S10.1 typed action `app.observe_in_sandbox`       | new typed action added to the S10.1 dispatch table; **queued** for S10.1 next-wave consolidation.                         |
| Manifest delta proposals                       | S10.1 typed action `app.propose_manifest_delta`   | new typed action; **queued** for S10.1 next-wave consolidation.                                                           |
| Recipe contribution                            | S10.1 typed action `app.contribute_recipe`        | new typed action; **queued** for S10.1 next-wave consolidation.                                                           |
| Capability lie audit                           | S11.1 §G first-run audit                          | already specified; this contract attaches `EcosystemRuntime` and `EcosystemHonestyClass` context.                         |
| Approval per app install                       | S5.3                                              | already exists; the approval prompt is extended (UI level; in scope for L7) to display ecosystem fields.                  |
| Recipe registry                                | S11.1 §3.2 `RepositoryKind = AIOS_COMMUNITY_REPO` | already exists; recipes are content-addressed objects in that repo.                                                       |
| Per-group namespace isolation                  | S8.1 + S4.1                                       | already exists; per-runtime path discipline (S3.2 §18.4) covers ecosystem runtime paths.                                  |
| Per-group GPU isolation                        | S8.2                                              | already exists; per-app `GpuPolicy` is composed by S3.2 §19 from the ecosystem runtime adapter default.                   |
| Network outbound manifest                      | S8.1 §G                                           | already exists; every app proposal carries a signed `NetworkOutboundManifest`.                                            |
| Evidence record types                          | S3.1 RecordType vocabulary                        | fourteen record types **queued** for S3.1 Wave 7 consolidation; see §13.                                                  |
| `SandboxProfile.ecosystem_runtime` typed field | S3.2 §3 `SandboxProfile`                          | one typed field **queued** for S3.2 Wave 7 consolidation; this contract does NOT modify S3.2 directly.                    |
| Constitutional honesty principle               | L0 invariants                                     | one candidate invariant `ECOSYSTEM_HONESTY_DISCLOSURE` **queued** narrative-only; this contract does NOT author it in L0. |

### 8.1 Required addition queued for S3.2 follow-up

S3.2 SandboxProfile gains one new typed field in a future consolidation wave (Wave 7):

```proto
// In aios.sandbox.v1alpha1.SandboxProfile, add:
EcosystemRuntime ecosystem_runtime = N;   // closed enum from S12.1 (this contract)
```

This field carries the runtime selection from app install through to sandbox composition. The composition algorithm gains a new merge rule: the runtime adapter's declared default profile (`AdapterDefault` source) MUST agree with the manifest-declared `ecosystem_runtime`; mismatch is a `CompositionError.code = INVALID_INPUT_SCHEMA` with sub-reason `EcosystemRuntimeAdapterMismatch`.

This contract **does not** modify S3.2 — the orchestrator integrates the field in Wave 7. The field is referenced narratively here so consumers know it is coming.

### 8.2 Required additions queued for S10.1 follow-up

S10.1 gains four new typed actions in the same Wave 7 consolidation:

| Typed action id              | Dispatch           | Subject discipline                                              |
| ---------------------------- | ------------------ | --------------------------------------------------------------- |
| `app.observe_in_sandbox`     | `ISOLATED_SANDBOX` | `_system:service:app-observer` (system service identity)        |
| `app.translate_manifest`     | `ISOLATED_SANDBOX` | AI subject (`is_ai = true`); INV-002 binds                      |
| `app.propose_manifest_delta` | `ISOLATED_SANDBOX` | AI subject; INV-002 binds                                       |
| `app.contribute_recipe`      | `ISOLATED_SANDBOX` | HUMAN_USER (operator-initiated; AI cannot contribute on behalf) |

This contract **does not** modify S10.1 — the orchestrator integrates the actions in Wave 7. The actions are referenced narratively.

### 8.3 Required additions queued for S3.1 follow-up

Fourteen evidence record types are queued for the S3.1 RecordType closed vocabulary; see §13.

## 9. Adversarial robustness

This section enumerates the named adversaries this contract addresses and how it addresses each one mechanically.

### 9.1 App modifying behaviour after the 60 s audit window

**Adversary:** an app behaves correctly during Phase C's 60 s window, then begins exercising undeclared capabilities later.

**Mitigation:** continuous monitoring extends past the 60 s window. Each ecosystem runtime adapter (system service identity) keeps a rolling structural-syscall counter at the Phase A `SyscallClass` granularity. A sustained pattern (≥ 5 distinct denial events for the same undeclared capability over a rolling 1-hour window) triggers a re-quarantine and `CAPABILITY_LIE_DETECTED` (FOREVER). The 60 s window remains the strict admission gate; sustained-pattern detection is the watchful eye after.

### 9.2 Wine prefix breakout

**Adversary:** a Win32 binary inside a Wine prefix tries to escape the prefix and reach the host.

**Mitigation:** S3.2 §9.1 already enforces that the Wine prefix is itself sandboxed by the AIOS sandbox composer — the Wine prefix cannot loosen the AIOS floor. This contract reinforces: any breakout attempt detected by the kernel-level sandbox enforcer (seccomp violation, attempted ptrace of host process, attempted access to `/aios/system/...`) emits `APP_ECOSYSTEM_RUNTIME_BREAKOUT_ATTEMPTED` (FOREVER) and immediately quarantines the app. The operator cannot un-quarantine without S5.3 approval and the FOREVER evidence record remains.

### 9.3 Waydroid container breakout

**Adversary:** an Android app inside a Waydroid container tries to break out and access the host filesystem or another group's namespace.

**Mitigation:** the Waydroid container runs under the per-group AIOS namespace + S8.2 per-group VkDevice partition. A successful breakout from the container does not grant access to `/aios/groups/<other_group>/...` — INV-011 (cross-group access forbidden) is enforced at the namespace and policy layers, not just at the Waydroid wrapper. `APP_ECOSYSTEM_RUNTIME_BREAKOUT_ATTEMPTED` evidence emits regardless of whether the breakout succeeded in escaping the container.

### 9.4 Malicious recipe in the community registry

**Adversary:** an attacker uploads a recipe whose declared SandboxProfile is broader than the app actually needs (or whose `NetworkOutboundManifest` allows exfiltration endpoints), trusting that operators will accept the recipe and that the app's actual behaviour is benign.

**Mitigation:** the local first-run audit (Phase C) compares declared vs observed. A recipe whose declared capabilities exceed the app's actual usage does NOT trigger the lie audit (the lie audit fires on _more_ observed than declared, not less). However:

- the L7 marketplace surface displays the declared capability set at the approval prompt — broad declarations are operator-visible;
- the registry's reputation tracks `manifest_delta_rejected_count` — recipes whose declared capabilities are repeatedly rejected by operators in favor of narrower deltas accumulate a poor reputation;
- the `RecipeTrustClass` is `RECIPE_COMMUNITY` for individually contributed recipes — the L7 marketplace UI surfaces the trust class prominently so operators are not silently led to over-broad recipes.

The defense against an over-broad recipe is not the audit — it is informed operator consent.

The defense against an under-broad recipe (declared capabilities < observed) is the standard Phase C capability-lie audit; the recipe is quarantined and its `capability_lie_event_count` increments per affected install.

### 9.5 App lying about EcosystemHonestyClass

**Adversary:** a recipe claims `FULLY_SUPPORTED` for an app that is actually `NOT_RUNNABLE_ON_NON_NATIVE` on the operator's hardware (e.g., a recipe claims it can run an iOS-only app via Wine — which it cannot).

**Mitigation:** the runtime detects behaviour inconsistent with the claimed honesty class:

- a `FULLY_SUPPORTED`-claimed app whose Phase A observation shows it terminating immediately with hardware-attestation failure;
- a `FULLY_SUPPORTED`-claimed app whose Phase C observation shows persistent core-feature denial despite a fully approved manifest (e.g., it cannot access its own runtime because the runtime cannot be created on this hardware).

In either case, `APP_HONESTY_CLASS_VIOLATION` (FOREVER) is emitted and the recipe's `capability_lie_event_count` increments.

### 9.6 iOS-claim attack (registry ingest)

**Adversary:** someone uploads a recipe to the community registry claiming "we can run iOS apps directly on x86 Linux" with `RUNTIME_LINUX_NATIVE` or `RUNTIME_WINDOWS_PROTON` and a strategy other than `IOS_REMOTE_BRIDGE`.

**Mitigation:** registry ingest enforces a structural rule:

```text
IF recipe.strategy == IOS_REMOTE_BRIDGE THEN recipe.ecosystem_runtime MUST equal RUNTIME_REMOTE_APPLE_BRIDGE
IF recipe.ecosystem_runtime == RUNTIME_REMOTE_APPLE_BRIDGE THEN recipe.strategy MUST equal IOS_REMOTE_BRIDGE
```

Any other combination involving iOS artifacts is rejected at ingest with `APP_RECIPE_DECEPTIVE_REJECTED_AT_INGEST` (FOREVER retention) — the rejection is permanent and audit-visible. Additionally, an artifact whose magic bytes match an iOS Mach-O ARM64 binary submitted to a Phase A observation under any non-`IOS_REMOTE_BRIDGE` strategy is rejected at the Phase A entry point with the same evidence record.

### 9.7 Anti-cheat game forging Wine detection

**Adversary:** a game ships an anti-cheat that refuses to run under Wine; an operator (or recipe) attempts to convince AIOS to falsify a Windows-native fingerprint to fool the anti-cheat.

**Mitigation:** AIOS reports honestly per-game. The recipe's `compatibility_caveats` field documents that the anti-cheat refuses Wine. AIOS does not falsify a Windows-native fingerprint — that would be supply-chain dishonesty (the game would believe it is running on a Microsoft-attested Windows kernel when it is not), inviting the operator to violate the game's TOS, and breaking the constitutional honesty principle. The honest answer is: "this game's anti-cheat refuses Wine; switch to `RUNTIME_WINDOWS_VM` if you want to play it." `APP_RECIPE_DECEPTIVE_REJECTED_AT_INGEST` (FOREVER) is emitted for any recipe that ships a Windows-native-fingerprint-spoofing payload as part of its sandbox profile or runtime adapter override.

### 9.8 Recipe re-publish after deplatform

**Adversary:** a publisher whose recipes were quarantined or whose publisher key was deplatformed (S11.1 §3.1 `DEPLATFORMED`) creates a new identity and re-publishes the same malicious recipe under a different vendor name and signing key.

**Mitigation:** the registry binds each recipe to a `contributor_subject_canonical_id` and the publisher's `publisher_root_id` (S11.1 §3.1) when the contributor is a publisher. The publisher catalog at S11.1 §3.1 is AIOS-root-signed and resists takedown evasion. Additionally, the recipe content hash (`canonical_id`) is the primary key in the registry — a re-published recipe with the same content lands at the same `canonical_id` and inherits the existing reputation, including any quarantine state. A re-published recipe with cosmetic changes that produce a new `canonical_id` is still subject to local Phase A and Phase C audits, which catch the same capability lies that triggered the original quarantine. The defense-in-depth chain runs: publisher catalog (`PUBLISHER_DEPLATFORMED` blocks at admit) → content hash deduplication (re-publish under same hash inherits state) → local audit (catches the same lie).

### 9.9 Phase A breakout into the observer namespace

**Adversary:** an artifact submitted for Phase A observation attempts to escape the observer's max-restricted sandbox into the host or into another group's namespace.

**Mitigation:** the Phase A observer adapter (`_system:service:app-observer`) declares a profile floor that is stricter than any operator-facing app: `LOOPBACK_ONLY` network, `tmpfs_for_tmp`, `home_isolation`, `gpu_capability_class = GPU_NONE`, `deny_compute_pipeline`, `deny_validation_layers`, narrow seccomp, no user namespaces, no ptrace. The observer runs under `_system:service:app-observer` whose `is_ai = false` and `is_recovery_mode = false` — the AI-initiated floor of S3.2 §5.2 still applies for AI proposers in Phase B but Phase A itself is service-scoped. Per S3.2 §18.4, the observer's runtime path is `/aios/system/runtime/observer/<observation_id>/scratch`; per S3.2 §18.3, the apply-time group-ownership check rejects any cross-group target binding. A breakout attempt aborts observation, emits `APP_ECOSYSTEM_RUNTIME_BREAKOUT_ATTEMPTED` (FOREVER), and adds the artifact's `BLAKE3` content hash to a per-host blocklist that prevents further observation attempts of the same artifact bytes.

### 9.10 AI subject attempting direct install

**Adversary:** an AI subject with a bound install capability attempts to transition an install action through `policy_pending → executing`.

**Mitigation:** S0.1 envelope FSM (cited in INV-002) rejects the transition for any AI-submitted install action regardless of capability bindings. `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` (FOREVER) is emitted on every attempt. The AI subject can only emit a proposal that flows through Phase B → S5.3 approval → S11.1 install pipeline.

## 10. Telemetry contract

All metrics use bounded label cardinality. Subject id, app id, recipe id, observation id, and proposal id are NEVER labels — they appear in evidence records, never as Prometheus labels.

| Metric                                 | Type      | Labels (closed sets)                                                                                                |
| -------------------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------- |
| `app_install_total`                    | counter   | `result` (success/failure/quarantined), `ecosystem_runtime` (12-value enum), `honesty_class` (4-value enum)         |
| `app_observe_in_sandbox_total`         | counter   | `result` (success/timeout/breakout_aborted), `strategy` (8-value enum)                                              |
| `app_observe_duration_seconds`         | histogram | `strategy`                                                                                                          |
| `app_translate_manifest_total`         | counter   | `result` (proposed/approved/rejected/structural_failure), `strategy`                                                |
| `app_capability_lie_detected_total`    | counter   | `ecosystem_runtime`                                                                                                 |
| `app_honesty_class_violation_total`    | counter   | `claimed_class` (4-value enum), `observed_kind` (closed: HARDWARE_ATTESTATION_FAILED / RUNTIME_INIT_FAILED / OTHER) |
| `app_recipe_imported_total`            | counter   | `source` (PROTONDB / FLATHUB / AUR / SNAPCRAFT)                                                                     |
| `app_recipe_contributed_total`         | counter   | `ecosystem_runtime`                                                                                                 |
| `app_manifest_delta_proposed_total`    | counter   | `outcome` (proposed/approved/rejected/capability_lie)                                                               |
| `app_recipe_quarantined_total`         | counter   | `reason_class` (capability_lie_threshold / honesty_violation / breakout_attempted / operator_request)               |
| `app_runtime_breakout_attempted_total` | counter   | `ecosystem_runtime`                                                                                                 |
| `app_recipe_active_count`              | gauge     | `trust_class` (4-value enum), `ecosystem_runtime`                                                                   |

Cardinality budget: ≤ 200 active label tuples per metric. The `ecosystem_runtime` enum has 12 values; `honesty_class` has 4; `strategy` has 8; `trust_class` has 4. The product of any two is well under the budget.

## 11. Evidence record types (queued for S3.1 Wave 7)

The following fourteen record types are queued for the S3.1 RecordType closed vocabulary. This contract does NOT modify S3.1 — the orchestrator integrates these in Wave 7.

| Record type                                | Trigger                                                                                                             | Retention class |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------- | --------------- |
| `APP_OBSERVE_STARTED`                      | Phase A `app.observe_in_sandbox` action transitioned to `executing`.                                                | STANDARD_24M    |
| `APP_OBSERVE_COMPLETED`                    | Phase A action transitioned to `succeeded`; carries the `ObservedBehavior` summary.                                 | STANDARD_24M    |
| `APP_OBSERVE_TIMEOUT`                      | Phase A hard timeout reached; partial summary emitted.                                                              | EXTENDED_60M    |
| `APP_TRANSLATE_MANIFEST_PROPOSED`          | Phase B `app.translate_manifest` action emitted a proposal awaiting approval.                                       | STANDARD_24M    |
| `APP_TRANSLATE_MANIFEST_APPROVED`          | S5.3 approval granted; the proposal becomes the bound manifest for install.                                         | STANDARD_24M    |
| `APP_TRANSLATE_MANIFEST_REJECTED`          | S5.3 approval denied or expired; proposal discarded.                                                                | EXTENDED_60M    |
| `APP_RECIPE_CONTRIBUTED`                   | Operator contributed a recipe back to the registry.                                                                 | STANDARD_24M    |
| `APP_RECIPE_IMPORTED`                      | One-shot import from upstream (ProtonDB / Flathub / AUR / Snapcraft) completed.                                     | STANDARD_24M    |
| `APP_MANIFEST_DELTA_PROPOSED`              | Phase D delta proposal emitted.                                                                                     | STANDARD_24M    |
| `APP_MANIFEST_DELTA_APPROVED`              | Operator approved Phase D delta; new manifest version installed.                                                    | STANDARD_24M    |
| `APP_HONESTY_CLASS_VIOLATION`              | Runtime observed behaviour inconsistent with the claimed `EcosystemHonestyClass`.                                   | FOREVER         |
| `APP_ECOSYSTEM_RUNTIME_BREAKOUT_ATTEMPTED` | Wine breakout, Waydroid breakout, VM escape attempt, or any sandbox enforcer reporting an integrity violation.      | FOREVER         |
| `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED`  | AI subject tried to install without operator approval — INV-002 enforcement.                                        | FOREVER         |
| `APP_RECIPE_DECEPTIVE_REJECTED_AT_INGEST`  | Recipe claims iOS direct execution, ships fingerprint-spoofing payload, or otherwise lies about runtime capability. | FOREVER         |

Each record carries:

- `app_id` (or `proposal_id` / `recipe_id` as relevant);
- `ecosystem_runtime`, `honesty_class`, `strategy`;
- `observation_id` and `observed_behavior_hash` (for Phase A/D records);
- `policy_decision_id` and approval evidence id (for Phase B/D records);
- redacted observation summary where applicable (no raw secrets, no raw clipboard, no raw network payloads).

## 12. Worked examples

### 12.1 Steam game install (Hogwarts Legacy)

```text
Step 1: Operator searches catalog for "Hogwarts Legacy"
        L7 marketplace surface queries the registry and finds:
          recipe_id = recipe:steam:hogwarts-legacy:1.0.5
          ecosystem_runtime = RUNTIME_WINDOWS_PROTON
          honesty_class = PARTIALLY_SUPPORTED
          trust_class = RECIPE_COMMUNITY
          reputation.successful_install_count = 215
          reputation.capability_lie_event_count = 1   (well below 5% threshold)
          reputation.manifest_delta_approved_count = 18
          upstream_attribution = ["protondb:9281"]

Step 2: AI presents recipe + capabilities at the approval prompt:
          - filesystem read of game directory
          - network: *.steamcontent.com, *.steampowered.com (HTTPS only)
          - GPU class = GPU_FULL_3D
          - audio output (no microphone)
          - clipboard brokered
          - honesty disclosure: "Native Windows binary running under Wine/Proton.
            Proton-marked Platinum on ProtonDB (recipe attribution preserved).
            215 prior successful installs in the AIOS community registry."
          - compatibility_caveats: "Anti-cheat denied. Story mode supported."

Step 3: Operator approves. S5.3 EXACT_ACTION binding consumed.

Step 4: S11.1 install pipeline runs through admit → INSTALLING → ACTIVE.
        Wine prefix created at:
          /aios/groups/g_default/agents/_user_operator/runtime/wine/hogwarts-legacy/

Step 5: Phase C audit (60 s) — game launches, contacts Steam endpoints in the
        approved allow list, opens GPU compute pipeline (declared), no other
        capabilities exercised. Audit passes.

Step 6: Game runs. Operator plays for 30 hours.

Step 7: One month later, an upstream patch breaks the manifest (game now
        requires *.gameco-cdn.net for cloud saves). Phase D fires:
          - app.propose_manifest_delta proposes adding the new endpoint.
          - Evidence: "blocked DNS to gameco-cdn.net; matches ProtonDB
            recipe 9281 patch notes."
          - Operator approves. Manifest v2 signed and recorded.
          - APP_MANIFEST_DELTA_APPROVED emitted (STANDARD_24M).
          - Recipe reputation increments manifest_delta_approved_count.
```

### 12.2 Android note-taking app from APK side-loaded

```text
Step 1: Operator drops "ColorNotes-2.4.7.apk" into the install surface.
        Recipe lookup: NOT FOUND.

Step 2: Phase A fires automatically:
          subject = _system:service:app-observer
          dispatch = ISOLATED_SANDBOX
          observation_id = obs_01HZK9X...
          duration = 28 s (app started, attempted Google Play Services
                          contact, fell back to local-only mode, idle)
          ObservedBehavior summary:
            attempted_dns_resolutions = ["mtalk.google.com"]   (blocked)
            blocked_filesystem_writes = ["~/.android/cache"]
            attempted_audio_init = false
            attempted_microphone_open = false
            attempted_camera_open = false
            attempted_gpu_init = true (display only)
            process_terminated_normally = true
            exit_code = 0
        APP_OBSERVE_COMPLETED emitted (STANDARD_24M).

Step 3: Phase B fires:
          strategy = ANDROID_MANIFEST_XML
          AndroidManifest.xml declares: WRITE_EXTERNAL_STORAGE, INTERNET.
          Combined with ObservedBehavior:
            ecosystem_runtime = RUNTIME_ANDROID_WAYDROID
            honesty_class = PARTIALLY_SUPPORTED
            sandbox_profile.filesystem.allow_write =
              [ /aios/groups/g_default/users/u_op/notes/colornotes/ ]
            network_outbound_manifest = DENY_ALL
            declared_capabilities = ["display.gpu_basic", "filesystem.user_notes"]
            honesty_disclosure_text = "This Android app falls back to
              local-only because Google Play Services are not available
              in Waydroid. Network is denied because the app's online
              sync requires Google services."
          APP_TRANSLATE_MANIFEST_PROPOSED emitted (STANDARD_24M).

Step 4: Operator reviews. The narrow manifest matches the operator's
        expectation. Operator approves; APP_TRANSLATE_MANIFEST_APPROVED
        emitted (STANDARD_24M). EXACT_ACTION binding consumed.

Step 5: S11.1 install pipeline runs. App becomes ACTIVE.

Step 6: Phase C audit (60 s) — app starts in Waydroid, attempts no
        capabilities outside the declared set. Audit passes.
```

### 12.3 iOS app the user wants

```text
Step 1: Operator searches "Kindle for iOS" in the marketplace.

Step 2: Marketplace surface returns the honest answer:
          AIOS cannot run iOS apps on this hardware. Apple actively
          prevents this via secure enclave, entitlements, and hardware
          attestation. The honest options are:
            (a) Install Kindle Linux native (recipe found:
                recipe:amazon:kindle:linux-2024.7).
            (b) Use the remote Apple bridge (RUNTIME_REMOTE_APPLE_BRIDGE)
                if you own an iPhone or iPad. AIOS will display your
                device's Kindle app surface; the app runs on your device.
            (c) Use the Kindle web reader in the AIOS web renderer.

Step 3: Operator selects (a) Kindle Linux native.

Step 4: Standard flow runs:
          - Phase A observation runs against the Linux-native binary.
          - Phase B proposal: ecosystem_runtime = RUNTIME_LINUX_NATIVE,
            honesty_class = FULLY_SUPPORTED.
          - Operator approves.
          - Install completes.
          - Phase C audit passes.

Step 5: AIOS never pretended to run iOS. The honest "I cannot do this"
        was more trustworthy than a dishonest emulation that would
        have failed when the operator opened the app and saw a
        secure-enclave-attestation-failed dialog.
```

### 12.4 Recipe contribution after a successful install

```text
Step 1: An operator who successfully installed and ran "Open Source Audio
        Editor 3.5" (a Linux-native app) for two weeks decides to
        contribute the manifest back to the registry.

Step 2: L7 marketplace surface offers a "Contribute recipe" affordance.
        The operator selects it and confirms.

Step 3: A typed action `app.contribute_recipe` is constructed:
          subject = HUMAN_USER (operator's canonical id)
          dispatch = ISOLATED_SANDBOX
          inputs = {
            app_id, manifest_version,
            evidence_pack = {
              install_evidence_id (S5.3 approval evidence),
              phase_c_audit_evidence_id (clean audit),
              two-week runtime evidence: zero capability lies,
                                          zero quarantine events,
                                          zero Phase D delta proposals
            }
          }

Step 4: Approval prompt confirms the operator wants to publish. EXACT_ACTION
        bound to the contribution. Operator approves.

Step 5: A new AppRecipe is constructed with:
          contributor_subject_canonical_id = operator id (or anonymous
            ephemeral key derived from the operator's vault as in §6.2)
          trust_class = RECIPE_COMMUNITY
          reputation.successful_install_count = 1 (the contributor's own)
          upstream_attribution = []  (locally authored)

Step 6: Recipe is published to AIOS_COMMUNITY_REPO. APP_RECIPE_CONTRIBUTED
        emitted (STANDARD_24M). The recipe becomes searchable for other
        operators on the next index refresh.
```

### 12.5 Quarantine on capability lie

```text
Step 1: An installed app that was previously ACTIVE for two weeks begins
        attempting to read /etc/passwd via the Wine prefix. The action
        is denied by the sandbox (filesystem.deny includes /etc).

Step 2: After the fifth such denial in a rolling 1-hour window (the
        sustained-pattern threshold from §9.1), the post-window monitor
        fires:
          - app transitions to QUARANTINED.
          - CAPABILITY_LIE_DETECTED evidence emitted (FOREVER) with
            ecosystem_runtime = RUNTIME_WINDOWS_PROTON.
          - The recipe's reputation.capability_lie_event_count
            increments via the registry.
          - If capability_lie_event_count / successful_install_count
            now exceeds 0.05, the recipe is auto-promoted to
            RECIPE_QUARANTINED (no longer surfaced to operators except
            via explicit "show quarantined" filter).

Step 3: The operator is notified through the L7 admin surface. The
        operator can:
          (a) Inspect the FOREVER evidence record to understand what
              was attempted.
          (b) Uninstall the app (UNINSTALLING → REMOVED).
          (c) Override the quarantine via S5.3 + FOREVER evidence
              (requires explicit human approval; the override does not
              clear the existing FOREVER record).
          (d) Submit a Phase D delta if the operator believes the
              attempted access is legitimate (e.g., a Wine update
              changed the path the app uses for system-config probing);
              the delta flow is the standard path.

Step 4: If the app is removed, the recipe's quarantine status remains
        on the registry as audit-visible history. Future installs by
        other operators see the quarantine and the underlying evidence
        chain.
```

## 13. Acceptance criteria

- [ ] `EcosystemRuntime` is a closed enum with twelve values, exactly as enumerated in §3.1.
- [ ] `EcosystemHonestyClass` is a closed enum with four values: `FULLY_SUPPORTED`, `PARTIALLY_SUPPORTED`, `REQUIRES_VM`, `NOT_RUNNABLE_ON_NON_NATIVE`.
- [ ] `ManifestTranslationStrategy` is a closed enum with eight values, exactly as enumerated in §3.3.
- [ ] `ObservedBehavior` is a closed schema; `SyscallClass` is a closed enum with eleven values; redaction discipline (no raw secret bytes) is mandatory.
- [ ] `RecipeTrustClass` is a closed enum with four values; `ManifestDeltaOutcome` is a closed enum with four values.
- [ ] Phase A is a typed action `app.observe_in_sandbox` dispatched as `ISOLATED_SANDBOX` under the `_system:service:app-observer` subject; default time budget 30 s, hard cap 300 s.
- [ ] Phase B is a typed action `app.translate_manifest`; the proposer is an AI subject; INV-002 forbids AI from installing.
- [ ] Phase C reuses S11.1 §G first-run capability-lie audit and adds ecosystem-runtime context.
- [ ] Phase D is a typed action `app.propose_manifest_delta`; manifest widening is always operator-approved, never silent.
- [ ] Each `EcosystemRuntime` is a `PackageKind = ADAPTER` package (S11.1 §3.4); the runtime declares its default `SandboxProfile` floor.
- [ ] The Community Recipe Registry uses `RepositoryKind = AIOS_COMMUNITY_REPO` (S11.1 §3.2) and content-addressed `recipe:<vendor>:<app_name>:<version>` ids.
- [ ] Imported recipes (ProtonDB, Flathub, AUR, Snapcraft) are metadata-only; Phase A and Phase C always run locally; local observation always wins.
- [ ] The honesty principle is mandatory disclosure at every install prompt; `EcosystemHonestyClass` cannot be omitted.
- [ ] iOS-claim attacks are rejected at registry ingest with `APP_RECIPE_DECEPTIVE_REJECTED_AT_INGEST` FOREVER evidence; only `IOS_REMOTE_BRIDGE` strategy with `RUNTIME_REMOTE_APPLE_BRIDGE` runtime is admitted for iOS artifacts.
- [ ] Anti-cheat fingerprint spoofing is rejected at registry ingest with the same evidence type.
- [ ] AI subject install attempts emit `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` FOREVER and are rejected by the S0.1 envelope FSM.
- [ ] Wine prefix breakout, Waydroid breakout, and VM escape attempts emit `APP_ECOSYSTEM_RUNTIME_BREAKOUT_ATTEMPTED` FOREVER and quarantine the app.
- [ ] Sustained capability-lie patterns past the 60 s window trigger re-quarantine and `CAPABILITY_LIE_DETECTED` FOREVER.
- [ ] All paths under per-runtime sandbox profiles live under `/aios/groups/<group_id>/agents/<agent_id>/runtime/<runtime_kind>/...` per S3.2 §18.4 (or `/aios/system/runtime/...` for system service identities).
- [ ] Telemetry conforms to §10 cardinality bounds; subject/app/recipe/observation/proposal ids never appear as labels.
- [ ] The fourteen evidence record types in §11 are queued for S3.1 Wave 7 consolidation; the `SandboxProfile.ecosystem_runtime` field is queued for S3.2 Wave 7 consolidation; the four typed actions are queued for S10.1 Wave 7 consolidation; the candidate L0 invariant `ECOSYSTEM_HONESTY_DISCLOSURE` is queued narrative-only for next L0 revision.

## 13.1 Constitutional notes

This contract sits at the intersection of three constitutional commitments that AIOS makes to the operator. Each commitment is enforced by a different layer; this contract is the place where the three meet.

**Commitment 1 — bounded AI agency (INV-002, INV-013).** The AI proposes; the operator approves; the runtime executes. There is no path in this contract that lets an AI subject install an app or widen a manifest without explicit operator approval. Phase B emits proposals, never installs. Phase D emits delta proposals, never widenings. The S0.1 envelope FSM rejects AI-initiated installs at the FSM level; the rejection is mechanical, not advisory. Even the recipe contribution flow (§6.2) is operator-initiated — an AI can suggest a contribution as a normal proposal, but only the operator can publish.

**Commitment 2 — default-deny everywhere (INV-008, INV-017).** Every default in this contract is restrictive. Phase A's observer profile is stricter than any operator-facing app. Phase B-emitted proposals start from `NO_ACCESS` / `DENY_ALL` and grow only into what the artifact demonstrably needs — never into what it might want. The community registry's `RECIPE_COMMUNITY` trust class does not loosen the audit; it only loosens the marketing. The runtime safety floor of S3.2 still wins over every layer of this contract.

**Commitment 3 — honesty over reach (queued L0 invariant `ECOSYSTEM_HONESTY_DISCLOSURE`).** AIOS could pretend to run iOS apps. AIOS could falsify a Windows-native fingerprint to fool an anti-cheat. AIOS could ship a recipe claiming `FULLY_SUPPORTED` for an app it knows is `NOT_RUNNABLE_ON_NON_NATIVE`. None of those choices is technically prevented by L1 or L2 — they are prevented by this contract's choice to make honesty mechanical: every install prompt carries the honesty class, every observed-class violation emits FOREVER evidence, every iOS-claim recipe is rejected at registry ingest. The cost is that AIOS sometimes returns "I cannot do this on your hardware; here are the closest practical alternatives." The benefit is that when AIOS says "I can do this," the operator can trust that claim.

### 13.2 Per-runtime adapter manifest skeleton

Each `EcosystemRuntime` adapter is itself an S11.1 `PackageKind = ADAPTER` package. The adapter's manifest skeleton (illustrative, not normative wire format here — S11.1 §4 owns the normative `PackageManifest` shape) carries the runtime's declared default profile floor:

```yaml
package_kind: ADAPTER
package_name: aios-runtime-windows-proton
package_version: 9.0-3
publisher_trust_level: VERIFIED
ecosystem_runtime: RUNTIME_WINDOWS_PROTON # the runtime this adapter implements
honesty_class_inherited: PARTIALLY_SUPPORTED # adapter declares its honesty class
declared_capabilities:
  - "filesystem.read.runtime_image" # runtime binary read access
  - "process.spawn.wine_child"
  - "gpu.basic_3d" # adapter-level; per-app GPU class is composed
network_outbound_manifest:
  mode: DENY_ALL # the adapter itself does not call home
sandbox_profile_default: # this is the AdapterDefault feeding S3.2 §5.1
  filesystem:
    root_mode: READ_ONLY
    allow_write:
      - "/aios/groups/{group_id}/agents/{agent_id}/runtime/wine/{app_id}/prefix"
    deny:
      - "$HOME"
      - "/etc"
    tmpfs_for_tmp: true
    home_isolation: true
  network:
    mode: EXPLICIT_ALLOWLIST # per-app allow lists added at composition
    dns_brokered: true
    block_metadata_endpoints: true
  process:
    seccomp_profile_id: "wine-default"
    no_new_privileges: true
    drop_all_capabilities: true
    allow_ptrace: false
    allow_user_namespace: false
  resources:
    cpu_weight: 5000
    memory_max_bytes: 8589934592 # 8 GiB cap; per-app may tighten
    pids_max: 1024
  secrets:
    mode: NO_SECRET_ACCESS
  gpu_policy:
    gpu_capability_class: GPU_DISPLAY_3D
    deny_compute_pipeline: false # 3D allowed; compute gated by INV-024
    deny_validation_layers: false
  compatibility:
    kind: WINE_PROTON
    wine_proton:
      isolate_prefix: true
      block_host_home: true
      portal_file_picker: true
recovery_only: false
```

The skeleton above is the shape every runtime adapter manifest follows. The values change per runtime (the Waydroid adapter declares a Waydroid-shaped sandbox; the macOS-VM adapter declares VM resource shares; the remote-Apple-bridge adapter declares only the bridge endpoint and clipboard mediation), but the structure is identical so the install pipeline (S11.1 §5) and the sandbox composer (S3.2 §5) treat them uniformly.

### 13.3 Inputs the operator can feel

The contract above is mechanical, but the operator-facing experience must remain comprehensible. Three structural choices in this contract translate directly into operator-visible affordances that L7 marketplace and admin surfaces will render:

- **Recipe trust class is a colour.** `RECIPE_AIOS_CURATED` reads green; `RECIPE_COMMUNITY` reads neutral; `RECIPE_IMPORTED` reads neutral with the upstream source name; `RECIPE_QUARANTINED` reads warning-coloured and is hidden behind an explicit "show quarantined" toggle. The operator never has to read a JSON document to understand the chain of provenance.
- **Honesty class is one line of plain language.** `FULLY_SUPPORTED` reads "Native AIOS app." `PARTIALLY_SUPPORTED` reads "Works for many apps; specific issues are listed below." `REQUIRES_VM` reads "Runs in a virtual machine on your computer; uses about [N] GB RAM and [M] GB disk." `NOT_RUNNABLE_ON_NON_NATIVE` reads "AIOS cannot run [iOS/macOS/etc.] apps on this hardware. Closest options below." This wording is not normative — the L7 visual language sub-spec owns the exact text — but the structure (one line, plain Bulgarian/English, no jargon) is normative because it is the only way the honesty principle survives contact with a non-programmer operator.
- **Phase D never surprises the operator.** A manifest delta proposal arrives in the L7 admin queue with the same shape as an install prompt: declared change, evidence ("blocked DNS to gameco-cdn.net during last 14 days"), recipe attribution, approve/reject. There is no silent runtime widening; there is no "AIOS adjusted your manifest" notification after the fact.

These three choices are the contract's contribution to operator legibility. The L7 specs flesh out the visual language; this contract guarantees the underlying state model that L7 needs to render those affordances honestly.

## 14. Open deferrals

These are intentionally out of scope for S12.1 and tracked elsewhere:

- **Federation across hosts** — when a multi-host AIOS deployment exists, recipe sync between hosts (e.g., a per-tenant private recipe registry mirrored across the operator's machines) is desirable. Deferred (post-Rev.2; requires multi-host identity and federated namespace).
- **Cross-machine recipe sync** — beyond the simple "import metadata only" model, automatic propagation of locally-approved manifest deltas to the operator's other machines. Deferred.
- **Mobile companion app for AIOS recipes** — the operator may want to browse and pre-approve recipes from a phone. Deferred to L7 mobile renderer sub-spec.
- **Cross-runtime delegation** — an Android app inside Waydroid spawning a Linux-native helper inside the same AIOS environment. Currently each runtime is opaque to others. Deferred.
- **Kernel-level anti-cheat compatibility shims** — a structured way for `RUNTIME_WINDOWS_VM` to expose a verifier-friendly surface for anti-cheat without lying about the kernel. Deferred and likely impossible without vendor cooperation.
- **macOS legal grey-area policy** — under what jurisdictions and EULA terms `RUNTIME_MACOS_VM` is acceptable. Deferred to L10 distribution policy sub-spec.
- **Curated recipe quality scoring beyond reputation counters** — e.g., automated scoring of `compatibility_caveats` completeness. Deferred.
- **Recipe deprecation flow** — an upstream package version is end-of-life and its recipes should be retired without quarantining historical installs. Deferred.
- **Observation reproducibility** — Phase A observations are not yet bit-reproducible across hosts (different kernels, different glibc). Deferred to a future hardening wave.
- **Threshold signing of curated recipes** — multi-party signing for `RECIPE_AIOS_CURATED`. Deferred (mirrors S11.1's deferred multi-party AIOS root signing).

## See also

- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S3.2 — Sandbox Composition](04_sandbox_composition.md)
- [S4.1 — Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S5.3 — Approval Mechanics](../L4_Policy_Identity_Vault/04_approval_mechanics.md)
- [S6.4 — Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md)
- [S8.1 — Network Policy](../L8_Network_Hardware_Devices/02_network_policy.md)
- [S8.2 — GPU Resource Model](../L8_Network_Hardware_Devices/05_gpu_resource_model.md)
- [S10.1 — Capability Runtime gRPC](../L3_AIOS_SGR_Service_Graph_Runtime/03_capability_runtime_grpc.md)
- [S11.1 — Repository Model + Trust Roots](../L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [Rev.1 §17 — Application, Package, and Compatibility Model](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [L6 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

Status: REAL
Evidence: E1
