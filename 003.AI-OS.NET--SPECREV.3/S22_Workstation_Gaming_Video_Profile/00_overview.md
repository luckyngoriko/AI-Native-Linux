# S22 — Workstation, Gaming, and Video Profile

| Field     | Value                                                                                                                                                                                                                                                                  |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-29; awaiting implementation evidence)                                                                                                                                                                                                      |
| Phase tag | S22                                                                                                                                                                                                                                                                    |
| Layer     | Cross-cutting: L6, L7, L8 crossing L4, L9                                                                                                                                                                                                                              |
| Consumes  | S17 App Capsule Runtime, S8.2 GPU Resource Model, S8.3 Hardware Graph, S19 Driver and Firmware Capsule Plane, S2.3 Policy Kernel, S3.1 Evidence Log, S16.1 Security Profile Matrix                                                                                     |
| Produces  | `WorkstationPassport`, `GamePassport`, `VideoPassport`, `EnergyPolicy`/`PowerBudget`, `WorkstationFitChecker`, `GameRuntimeSelector`, `VideoEngineScheduler`, anti-cheat honesty classification, per-game sandbox/GPU grant, workstation/gaming/video evidence records |

## 1. Responsibility

S22 is the near-term differentiator plane. It turns "AIOS runs on a real
workstation, plays real games, and handles real video" from a marketing promise
into a typed, policy-governed, evidence-backed product surface.

S22 owns three product directions and one cross-cutting concern:

```text
workstation-agnostic  -> WorkstationPassport + WorkstationFitChecker + role profiles
game-agnostic         -> GamePassport + GameRuntimeSelector + gaming modes
super-video plane     -> VideoPassport + media capability classes + VideoEngineScheduler
energy / power policy -> EnergyPolicy + PowerBudget (homed here per DEC-R3-011)
```

S22 does not invent a new execution mechanism. Every game, capture, encode, and
power action is either an S17 `AppCapsule` launch, an S8.2 GPU capability binding,
an S8.3 hardware match, an S19 driver decision, or a typed action the S2.3 Policy
Kernel decides and the Capability Runtime (S10.1) executes. S22 adds the
domain-specific passports, selectors, schedulers, and gates that make those
mechanisms safe and explainable for high-performance interactive workloads.

Invariant links: INV-002 (AI proposes, never executes), INV-004 (sandbox
boundary), INV-005 (recovery boundary), INV-008 (evidence append-only),
INV-013 (AI-blocked privileged device control), INV-017 (no profile weakening),
INV-024 (capability honesty / no capability lie). New invariant INV-030
(workspace data-boundary isolation) is proposed by this contract; see §13.

## 2. Product principle

A workstation, a game, and a video pipeline are high-value, high-blast-radius
workloads. They demand GPU access, device passthrough, network endpoints, and
real-time scheduling — exactly the surfaces that can leak data or break the host.
S22's principle: make these workloads first-class and delightful, but never let
performance silently buy down isolation, evidence, or the recovery boundary.

```text
operator goal (run role / launch game / start capture)
  -> inspect signed state (WorkstationPassport / HardwareGraph / SecurityProfile)
  -> generate candidates (fit candidates / runtime candidates / capability classes)
  -> score benefit / risk / compatibility
  -> show risk diff (what GPU/data/network/power is touched)
  -> apply policy (S2.3) and required approval
  -> test off the active system where possible (fit dry-run / shader warm / probe)
  -> promote only with evidence
  -> rollback / degrade / route-to-VM / block with reason
```

This is the universal Rev.3 solver pattern (holistic §6). S22 reuses it; it does
not fork a second one. The defining gate is the workspace data boundary: a gaming
workspace cannot read work or family data, and a video capture cannot start
without visible consent. Performance modes raise priority and power, never trust.

## 3. Reference patterns

| Pattern                                                                                                                       | S22 use                                                                    |
| ----------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| [PipeWire](https://docs.pipewire.org/)                                                                                        | Wayland-safe audio/video graph; portal-mediated screen and camera capture. |
| [GStreamer](https://gstreamer.freedesktop.org/documentation/)                                                                 | Structured media pipelines for capture, encode, transcode, and streaming.  |
| [FFmpeg / libav](https://ffmpeg.org/documentation.html)                                                                       | Codec tooling, conversion, and probe of decode/encode capability.          |
| [VA-API](https://intel.github.io/libva/)                                                                                      | Vendor-neutral hardware decode/encode probe and binding.                   |
| [Vulkan Video](https://www.khronos.org/blog/an-introduction-to-vulkan-video)                                                  | Cross-vendor hardware video pipeline where the GPU and driver support it.  |
| [xdg-desktop-portal ScreenCast](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html) | Surface-scoped screen capture consent and PipeWire stream brokering.       |
| [Proton / Steam Linux Runtime](https://github.com/ValveSoftware/Proton)                                                       | Windows-game compatibility runtime candidate, per-game prefix isolation.   |
| [Lutris](https://lutris.net/) / [Heroic](https://heroicgameslauncher.com/)                                                    | Source-adapter model for GOG/Epic/Amazon and Wine recipe import.           |
| [Waydroid](https://docs.waydro.id/)                                                                                           | Android game runtime route (aligns with DEC-R3-004).                       |
| [gamescope](https://github.com/ValveSoftware/gamescope)                                                                       | Fullscreen/couch session compositor under AIOS security chrome.            |
| [WebRTC](https://webrtc.org/)                                                                                                 | Low-latency LAN-first game/remote-desktop streaming transport.             |
| [Linux kernel power management](https://docs.kernel.org/admin-guide/pm/index.html)                                            | EPP/cpufreq/platform-profile basis for `EnergyPolicy` enforcement.         |

External-stack risks are honest non-goals (§12): DRM and kernel-level anti-cheat
may refuse Linux entirely, and patent-encumbered codecs (HEVC, some AV1 hardware
paths) may be unavailable on a given build — S22 reports this, it does not pretend.

## 4. WorkstationPassport

The `WorkstationPassport` is the signed, content-addressed truth object for one
machine's shape and role. It is derived from the S8.3 `HardwareGraph`, the S8.2
GPU topology, the active S16.1 `SecurityProfile`, and the operator-chosen role.
AI and renderers read this before any fit decision; raw shell probing is fallback.

```yaml
workstation_passport:
  passport_id: "wsp_<ULID>"
  hardware_graph_snapshot_id: "hwgraph_<hex32>" # S8.3 binding
  workstation_class: DESKTOP_WORKSTATION # WorkstationClass enum, §4.1
  hardware_profile:
    cpu_class: "x86_64|aarch64|riscv64"
    core_count: 0
    ram_bytes: 0
    chassis: "desktop|laptop|sff|server|embedded"
    iommu_present: true
  gpu_profile: # derived from S8.2 GpuDevice topology
    vendor: "amd|intel|nvidia|software"
    capability_class: "GPU_NONE|GPU_SOFTWARE|GPU_BASIC|GPU_ACCEL|GPU_PASSTHROUGH"
    vram_bytes: 0
    apis: ["vulkan", "opengl", "webgpu"]
    isolation_state: "shared|partitioned|passthrough"
    driver_capsule_id: "drvcap_<ULID>|none" # S19 binding
  display_profile:
    monitors: 0
    max_refresh_hz: 0
    hdr: false
    vrr: false
    fractional_scaling: false
    remote_stream_capable: false
  input_profile:
    keyboard: true
    pointer: true
    touch: false
    stylus: false
    gamepad: false
    spacemouse: false
  power_profile: # feeds EnergyPolicy, §9
    source: "ac|battery|ups"
    battery_present: false
    thermal_budget_class: "unconstrained|managed|throttled"
    energy_policy_id: "enp_<ULID>|default"
  security_profile: SECURE_DEFAULT # S16.1 SecurityProfile enum
  workload_profile:
    roles: [BUSINESS] # WorkloadRole enum, §4.2
    active_workspace: "work|gaming|lab|family|admin"
  lifecycle:
    state: ACTIVE # PassportState enum, §4.3
    signature_chain: []
  evidence:
    fit_receipt: "evr_..."
    drift_receipt: "evr_..."
```

### 4.1 WorkstationClass enum (CLOSED)

```text
WorkstationClass =
  MOBILE_LAPTOP
| DESKTOP_WORKSTATION
| DEV_WORKSTATION
| CREATOR_WORKSTATION
| CAD_ENGINEERING
| TRADING_OPERATIONS
| SECURE_ADMIN_STATION
| THIN_CLIENT
| KIOSK_PUBLIC
| HEADLESS_WORKSTATION
```

Unknown values are rejected by the WorkstationPassport loader.

### 4.2 WorkloadRole enum (CLOSED)

```text
WorkloadRole =
  BUSINESS
| DEV
| CREATOR
| CAD
| TRADING
| GAMING
| REALTIME
| ADMIN
```

Unknown values are rejected by the WorkstationPassport loader. `SECURE_ADMIN_STATION`
and `KIOSK_PUBLIC` classes reject the `GAMING` role unless an expiring,
operator-approved exception is registered.

### 4.3 PassportState enum (CLOSED)

```text
PassportState =
  PROBING
| DRAFT
| ACTIVE
| DRIFTED
| RETIRED
```

Unknown values are rejected by the WorkstationPassport loader. A passport enters
`DRIFTED` when the S8.3 `HardwareGraph` cross-boot diff signals GPU/monitor/TPM/
input change; a drifted passport must be re-fit before role-gated launches proceed.

### 4.4 Workstation Fit Checker

The `WorkstationFitChecker` answers "can this machine run this role safely and
well?" It is a solver instance, not a new mechanism.

Inputs: `WorkstationPassport`, requested `WorkloadRole`, active `SecurityProfile`,
S8.2 GPU capability class, S19 driver state, `EnergyPolicy`.

```text
WorkstationFitVerdict =
  FIT_GOOD
| FIT_DEGRADED
| FIT_NEEDS_DRIVER
| FIT_NEEDS_HARDWARE
| FIT_BLOCKED_BY_PROFILE
| FIT_UNFIT
```

Unknown values are rejected by the WorkstationFitChecker.

Each verdict carries benefit, exact risk, touched devices/data/network, the
required driver or hardware delta, the rollback path, and the blocked reason.
`FIT_NEEDS_DRIVER` hands off to the S19 DriverSolver; it never installs a driver
itself. The verdict is recorded as `WORKSTATION_FIT_CHECKED`.

## 5. GamePassport and game model

A game is treated as a specialized `AppCapsule` (S17), never as a privileged host
mutation. The `GamePassport` is the game-specific truth object layered on the
capsule; the capsule provides isolation, rollback, and evidence, while the
passport provides GPU/input/network/save/mod/anti-cheat specifics.

```yaml
game_passport:
  passport_id: "gmp_<ULID>"
  capsule_id: "appcap_<ULID>" # S17 AppCapsule binding
  title: "example"
  source: STEAM # GameSource enum, §5.1
  runtime_selected: PROTON_STABLE # GameRuntimeSelector output, §5.2
  gaming_mode: SECURE_GAMING # GamingMode enum, §5.3
  compatibility: WORKS # GameCompatibility enum, §5.4
  anticheat:
    classification: PROTON_SUPPORTED # AntiCheatClass enum, §6
    honesty_note: "vendor enables EAC on Proton"
  gpu_grant: # per-game GPU grant, §7
    capability_class: GPU_ACCEL # S8.2 GpuCapabilityClass
    vram_budget_bytes: 0
    performance_mode: false
    thermal_ceiling_c: 0
  input_profile:
    gamepad: true
    keyboard_mouse: true
    gyro: false
    vr: false
  network_manifest: # per-game allowlist
    multiplayer_endpoints: []
    launcher_endpoints: []
    cloudsave_endpoints: []
    telemetry_allowed: false
  save_state:
    local_path_scope: "capsule-private"
    cloud_sync: false
    backup_receipt: "evr_...|none"
  mods:
    mod_manager: "none|managed"
    per_mod_sandbox: true
  data_boundary:
    may_read_work_data: false # INV-030; always false for gaming
    may_read_family_data: false
  evidence:
    runtime_receipt: "evr_..."
    anticheat_receipt: "evr_..."
```

### 5.1 GameSource enum (CLOSED)

```text
GameSource =
  STEAM
| GOG
| EPIC
| AMAZON
| ITCH
| HUMBLE
| EMULATOR
| BROWSER_CLOUD
| ANDROID
| WINDOWS_ANTICHEAT
```

Unknown values are rejected by the GamePassport loader. Each source maps to a
typed source adapter that reuses the S21 Package Rosetta intake path rather than
forking a second installer; emulator media and Android packages are user-owned
content with explicit legal disclosure.

### 5.2 GameRuntimeSelector

The `GameRuntimeSelector` is a solver instance over `GamePassport` candidates.

Inputs: source metadata, S8.2 GPU capability class, S19 driver state,
`SecurityProfile`, anti-cheat classification, active `GamingMode`.

```text
GameRuntime =
  NATIVE_LINUX
| STEAM_LINUX_RUNTIME
| PROTON_STABLE
| PROTON_EXPERIMENTAL
| PROTON_GE
| WINE
| LUTRIS_RECIPE
| EMULATOR_RUNTIME
| ANDROID_WAYDROID
| WINDOWS_VM
| CLOUD_WEB
| BLOCKED_RUNTIME
```

Unknown values are rejected by the GameRuntimeSelector. The selector emits a
typed decision with benefit, risk, compatibility, and fallback, recorded as
`GAME_RUNTIME_SELECTED`. Windows games run in per-game Wine/Proton prefixes
(never shared `~/.wine`) or `WINDOWS_VM`, consistent with holistic §7.

### 5.3 GamingMode enum (CLOSED)

```text
GamingMode =
  SECURE_GAMING
| PERFORMANCE_GAMING
| COMPATIBILITY_GAMING
| VM_GAMING
| KIDS_GAMING
| LAN_PARTY
```

Unknown values are rejected by the GamePassport loader.

| Mode                   | Effect                                                           | Required approval                                  |
| ---------------------- | ---------------------------------------------------------------- | -------------------------------------------------- |
| `SECURE_GAMING`        | Sandboxed, native/Proton, no personal data, default.             | None beyond launch.                                |
| `PERFORMANCE_GAMING`   | Raised GPU/CPU priority, higher power budget, thermal evidence.  | Operator approval; emits `ENERGY_BUDGET_ENFORCED`. |
| `COMPATIBILITY_GAMING` | More permissive runtime for old titles, still isolated.          | Operator approval.                                 |
| `VM_GAMING`            | Windows VM for kernel-anti-cheat / hard-DRM cases.               | Operator approval; passthrough gated by S8.2/S19.  |
| `KIDS_GAMING`          | Curated set, time/network limits, no store purchases by default. | Operator (guardian) approval.                      |
| `LAN_PARTY`            | Temporary LAN exposure with TTL.                                 | Operator approval; auto-expires with evidence.     |

### 5.4 GameCompatibility enum (CLOSED)

```text
GameCompatibility =
  WORKS
| WORKS_WITH_TWEAKS
| VM_ONLY
| ANTICHEAT_UNSUPPORTED
| DRM_UNSUPPORTED
| BLOCKED
```

Unknown values are rejected by the GamePassport loader.

## 6. Anti-cheat honesty classification

S22 must never promise support a vendor blocks. Every game with anti-cheat is
classified honestly and the classification is recorded as evidence.

```text
AntiCheatClass =
  NO_ANTICHEAT
| PROTON_SUPPORTED
| PROTON_OPT_IN_REQUIRED
| VM_ONLY
| WINDOWS_DUAL_BOOT_ONLY
| KERNEL_ANTICHEAT_BLOCKED
| UNKNOWN_ANTICHEAT
```

Unknown values are rejected by the GamePassport loader.

`AntiCheatClass` is the **game-runtime-selection view**, derived from the canonical
**per-capsule** Windows truth `AntiCheatStatus` owned by
[S17.3 §11](../S17_App_Capsule_Runtime/03_windows_capsule_runtime.md). S22 does not
re-decide anti-cheat support; it projects S17.3's status onto a runtime-selection class:

| S17.3 `AntiCheatStatus` (canonical) | S22 `AntiCheatClass` (selection view)                       |
| ----------------------------------- | ----------------------------------------------------------- |
| `SUPPORTED`                         | `PROTON_SUPPORTED`                                          |
| `OFFLINE_ONLY`                      | `PROTON_OPT_IN_REQUIRED`                                    |
| `BLOCKED_KERNEL_DRIVER`             | `KERNEL_ANTICHEAT_BLOCKED`                                  |
| `BLOCKED_VM_HOSTILE`                | `WINDOWS_DUAL_BOOT_ONLY`                                    |
| `BLOCKED_DRM`                       | `VM_ONLY`                                                   |
| `BLOCKED_VENDOR_REFUSES`            | `VM_ONLY` or `KERNEL_ANTICHEAT_BLOCKED` (per vendor reason) |
| `UNKNOWN`                           | `UNKNOWN_ANTICHEAT`                                         |
| (no anti-cheat present)             | `NO_ANTICHEAT`                                              |

The two enums never disagree on the safety outcome: any S17.3 `BLOCKED_*` status maps to a
VM/dual-boot/blocked selection, never to a host-kernel module. Rules:

- `KERNEL_ANTICHEAT_BLOCKED` games never receive a kernel module to satisfy them;
  the honest verdict is `WINDOWS_VM` route or `BLOCKED`. A game requesting a kernel
  driver is a hard deny on the host kernel (a game cannot author or load kernel
  modules; that is the S19 driver plane's privileged path, not a game's).
- The classification and its rationale are surfaced to the operator before launch
  and recorded as `GAME_ANTICHEAT_CLASSIFIED`.
- AI subjects may explain the classification; they cannot upgrade it or weaken the
  profile to make a blocked game run (INV-017).

## 7. Per-game sandbox and per-game GPU grant

Per-game sandbox (reuses S17 capsule isolation + S3.2 sandbox composition):

- A game capsule's filesystem scope is capsule-private; it cannot read the work,
  family, or admin workspace data (INV-030, §13).
- Network is restricted to the game's `network_manifest` allowlist; endpoints
  outside it are blocked with reason `network endpoint outside manifest`.
- Cloud-save and store credentials stay in the Vault Broker; the game never sees
  raw secrets (consistent with the secrets-as-capabilities canon).
- Mods are packages with their own trust, capability, rollback, and evidence; a
  mod requesting an unsafe filesystem write outside its sandbox is blocked.

Per-game GPU grant (binds S8.2 `GpuCapabilityBinding`):

- The grant names a `GpuCapabilityClass`, a VRAM budget, a performance flag, and a
  thermal ceiling. It is a scoped binding, not blanket GPU root.
- `PERFORMANCE_GAMING` raises the grant only after operator approval and emits a
  power-budget evidence record.
- Shader cache is per-game and versioned with cleanup/rollback; it is not shared
  across capsules.

## 8. VideoPassport and media capability classes

Video is a first-class capability plane: one policy/evidence model for playback,
capture, conferencing, recording, streaming, remote desktop, creator pipelines,
and surveillance. The `VideoPassport` is probed from S8.2 GPU video engines and
the S19 driver state.

```yaml
video_passport:
  passport_id: "vdp_<ULID>"
  hardware_graph_snapshot_id: "hwgraph_<hex32>" # S8.3 binding
  decode_paths: ["software", "vaapi", "qsv", "nvdec", "vcn", "vulkan_video"]
  encode_paths: ["software", "vaapi", "qsv", "nvenc", "vcn", "vulkan_video"]
  codecs:
    h264: true
    hevc: false # may be unavailable (patents), §12
    av1: false
    vp9: true
  display_features:
    hdr: false
    vrr: false
    color_management: false
  capture_paths: ["pipewire_portal", "camera_portal"]
  granted_classes: [VIDEO_PLAYBACK_BASIC] # VideoCapabilityClass enum, §8.1
  latency:
    decode_ms: 0
    encode_ms: 0
    dropped_frames: 0
  privacy:
    visible_indicator_required: true
    retention_class: "none|bounded|extended"
    workspace_boundary: "work|gaming|lab|family|admin"
  evidence:
    probe_receipt: "evr_..."
    capture_consent_receipt: "evr_...|none"
```

### 8.1 VideoCapabilityClass enum (CLOSED)

```text
VideoCapabilityClass =
  VIDEO_PLAYBACK_BASIC
| VIDEO_PLAYBACK_HW
| VIDEO_ENCODE_BASIC
| VIDEO_ENCODE_HW
| SCREEN_CAPTURE
| CAMERA_CAPTURE
| VIDEO_CONFERENCE
| GAME_STREAMING
| REMOTE_DESKTOP_STREAM
| CREATOR_PIPELINE
| SURVEILLANCE_FEED
```

Unknown values are rejected by the VideoPassport loader.

### 8.2 Video policy rules

- `SCREEN_CAPTURE`, `CAMERA_CAPTURE`, and `VIDEO_CONFERENCE` always require a
  visible UI indicator and explicit consent recorded as `VIDEO_CAPTURE_CONSENTED`.
- AI subjects can never request camera or screen capture silently; an AI capture
  request is a typed proposal that requires human approval (INV-002, INV-013).
- Screen capture is surface/window-scoped by default, never full-desktop.
- The work workspace may forbid capture or watermark captured streams; admin and
  recovery surfaces are non-capturable except by recovery-approved evidence capture.
- DRM-protected content that cannot be captured/streamed yields an honest blocked
  reason; S22 does not strip DRM.

### 8.3 VideoEngineScheduler

Hardware video engines (NVENC/QSV/VA/VCN/Vulkan Video) are scarce. The
`VideoEngineScheduler` arbitrates concurrent video workloads under power and
thermal caps. It is an L8 scheduler binding S8.2 GPU accounting, not a new
authority.

Inputs: active video workloads, `VideoPassport`, `EnergyPolicy`/`PowerBudget`,
`SecurityProfile`, GPU video-engine contention from S8.2.

```text
VideoEnginePriority =          # closed ordering, highest first
  RECOVERY_ADMIN_SHARE
| VIDEO_CONFERENCE_LIVE
| GAME_STREAMING
| REMOTE_WORKSTATION_STREAM
| SURVEILLANCE_INGEST
| BACKGROUND_TRANSCODE
```

Unknown values are rejected by the VideoEngineScheduler. The scheduler decides
priority, codec, hardware-vs-software fallback, bitrate/framerate/resolution caps,
thermal/power cap, and retention. When a hardware encoder is throttled, background
transcode is paused first and the decision is recorded as `VIDEO_CAPABILITY_PROBED`
(with a contention/throttle note). The "why-is-video-broken" diagnostic maps each
blocker (codec / driver / policy / portal / bandwidth / thermal / DRM / workspace)
to its owning plane and a plain-language reason.

## 9. EnergyPolicy and PowerBudget (DEC-R3-011)

S22 homes energy/power policy per DEC-R3-011. `EnergyPolicy` binds per-app energy
budgets and battery-mode capability restrictions to the kernel power-management
substrate; `PowerBudget` is the per-target enforced envelope.

```yaml
energy_policy:
  policy_id: "enp_<ULID>"
  power_source: "ac|battery|ups"
  platform_profile: "performance|balanced|low-power" # kernel platform-profile
  epp_hint: "performance|balanced_performance|balanced_power|power"
  battery_mode_restrictions:
    block_performance_gaming: true
    block_hw_encode_background: true
    cap_remote_stream_bitrate: true
  thermal:
    ceiling_c: 0
    throttle_action: "reduce_priority|pause_background|degrade_quality"
  evidence:
    enforcement_receipt: "evr_..."

power_budget:
  budget_id: "pwb_<ULID>"
  target_kind: "capsule|game|video_workload|workstation"
  target_id: "appcap_..|gmp_..|vdp_..|wsp_.."
  cpu_priority_class: "idle|normal|elevated|realtime_capped"
  gpu_performance_mode: false
  watt_ceiling_estimate: 0
  enforced_under_profile: SECURE_DEFAULT
  energy_policy_id: "enp_<ULID>"
```

Rules:

- On `battery` power, `PERFORMANCE_GAMING` and background hardware encode are
  blocked unless the operator explicitly overrides; the override emits
  `ENERGY_BUDGET_ENFORCED`.
- A `realtime_capped` priority class never grants true `SCHED_FIFO` without going
  through the realtime workload path; S22 power budgets cannot escalate scheduling
  privilege on their own.
- Every enforced budget that changes priority, GPU performance mode, or thermal
  action emits `ENERGY_BUDGET_ENFORCED`.

## 10. Security profile gates

| Profile          | Workstation / Gaming / Video rule                                                                                                                    |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `DEV_RELAXED`    | All roles, all gaming modes, all capture classes allowed with warning and rollback.                                                                  |
| `SECURE_DEFAULT` | Gaming sandboxed by default; capture requires consent; performance modes require approval; `WINDOWS_VM` allowed.                                     |
| `STIG_ALIGNED`   | `GAMING` role and gaming modes blocked on `SECURE_ADMIN_STATION`; capture is recovery/owner-approved only; only signed runtimes; `LAN_PARTY` denied. |
| `AIRGAP_HIGH`    | No store/launcher network; no cloud save; no live runtime download; capture local-only; surveillance/remote-stream require explicit local approval.  |

Hard denies (Policy Kernel, all profiles unless noted):

- No game or video workload may read another workspace's data (INV-030).
- No game may load a kernel module or run a kernel-level anti-cheat on the host
  kernel; the honest answer is VM or blocked.
- No AI subject may approve a GPU grant, performance mode, capture, VM passthrough,
  or energy-budget override (INV-002, INV-013).
- No performance, gaming, or capture action may weaken the active `SecurityProfile`
  (INV-017).
- No camera or screen capture may start without a visible indicator and recorded
  consent.
- No `LAN_PARTY` exposure may persist past its TTL without re-approval.

## 11. Evidence records

S22 adds these record types:

```text
WORKSTATION_FIT_CHECKED
GAME_RUNTIME_SELECTED
GAME_ANTICHEAT_CLASSIFIED
VIDEO_CAPABILITY_PROBED
VIDEO_CAPTURE_CONSENTED
ENERGY_BUDGET_ENFORCED
```

Minimum fields for `WORKSTATION_FIT_CHECKED`:

```text
passport_id
hardware_graph_snapshot_id
requested_role
security_profile
gpu_capability_class
fit_verdict
benefit_summary
risk_summary
required_driver_delta
required_hardware_delta
rollback_plan_id
evidence_receipt_id
```

Minimum fields for `VIDEO_CAPTURE_CONSENTED`:

```text
video_passport_id
capability_class
requesting_subject_id
subject_kind
capture_scope
visible_indicator_shown
workspace_boundary
consent_method
security_profile
evidence_receipt_id
```

## 12. Non-goals

- Do not promise every game runs; anti-cheat and DRM vendors may refuse Linux,
  and S22 classifies that honestly rather than faking support.
- Do not load a kernel module or weaken the kernel to satisfy kernel-level
  anti-cheat; the honest path is VM or blocked.
- Do not strip or bypass DRM on protected video; report the blocked reason.
- Do not claim a patent-encumbered codec (e.g. HEVC, some hardware AV1 paths) is
  available when the build/hardware/policy does not provide it.
- Do not let a gaming, performance, or capture workload read work/family/admin
  data, escalate scheduling privilege, or weaken the security profile.
- Do not let AI silently grant GPU, capture, VM passthrough, or power overrides.
- Do not fork a second installer for game sources; reuse S21 package intake.
- Do not turn the recovery boundary off for performance; recovery survives without
  any of these workloads.

## 13. Proposed new invariant

This contract proposes one new constitutional rule for the Rev.3 invariant
register (`04_invariants.md`), continuing the INV-025+ sequence:

- **INV-030** — _Workspace data-boundary isolation._ A workload bound to one
  workspace (work, gaming, lab, family, admin) cannot read another workspace's
  data, secrets, or saves. A gaming workspace, in particular, can never read work
  or family data, regardless of performance mode or operator convenience.

The integrator must register INV-030 in `04_invariants.md` and map the prose rules
in §7 and §10 to it.

## 14. Acceptance criteria

S22 is `REAL` only when:

1. `WorkstationPassport` parses, binds an S8.3 `HardwareGraph` snapshot, and rejects
   unknown `WorkstationClass`, `WorkloadRole`, and `PassportState` values.
2. `WorkstationFitChecker` emits a typed `WorkstationFitVerdict` with benefit, risk,
   touched devices/data/network, required driver/hardware delta, and rollback path,
   recorded as `WORKSTATION_FIT_CHECKED`.
3. `GamePassport` is layered on an S17 `AppCapsule` and rejects unknown `GameSource`,
   `GameRuntime`, `GamingMode`, `GameCompatibility`, and `AntiCheatClass` values.
4. `GameRuntimeSelector` emits `GAME_RUNTIME_SELECTED` with a fallback for at least
   one native, one Proton, and one VM/blocked path.
5. Anti-cheat classification emits `GAME_ANTICHEAT_CLASSIFIED`; a
   `KERNEL_ANTICHEAT_BLOCKED` game is never satisfied with a host kernel module.
6. A gaming workspace cannot read work or family data (INV-030), proven by a denied
   cross-workspace read test.
7. Per-game GPU grant binds an S8.2 `GpuCapabilityClass` with a VRAM budget and never
   becomes blanket GPU access.
8. `VideoPassport` probes decode/encode/capture paths and rejects unknown
   `VideoCapabilityClass` values, recorded as `VIDEO_CAPABILITY_PROBED`.
9. Screen/camera capture cannot start without a visible indicator and recorded
   `VIDEO_CAPTURE_CONSENTED`; an AI silent-capture request is denied.
10. `VideoEngineScheduler` arbitrates concurrent workloads by closed
    `VideoEnginePriority` ordering under power/thermal caps.
11. On battery power, `PERFORMANCE_GAMING` and background hardware encode are blocked
    unless overridden, and the override emits `ENERGY_BUDGET_ENFORCED`.
12. No S22 action can weaken the active `SecurityProfile` or let an AI subject approve
    a GPU grant, performance mode, capture, VM passthrough, or energy override.

## 15. See also

- [S17 App Capsule Runtime](../S17_App_Capsule_Runtime/00_overview.md)
- [S19 Driver and Firmware Capsule Plane](../S19_Driver_Firmware_Capsule_Plane/00_overview.md)
- [S16.1 Security Profile Matrix](../S16_Security_Hardening_Compliance/01_security_profile_matrix.md)
- [S8.2 GPU Resource Model](../../002.AI-OS.NET--SPECREV.2/L8_Network_Hardware_Devices/05_gpu_resource_model.md)
- [S8.3 Hardware Graph](../../002.AI-OS.NET--SPECREV.2/L8_Network_Hardware_Devices/01_hardware_graph.md)
- [S2.3 Policy Kernel](../../002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S3.1 Evidence Log](../../002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/01_evidence_log.md)
- [Rev.3 Holistic Specification](../00_REV3_HOLISTIC_SPEC.md)
- [Rev.3 Design Decisions (DEC-R3-011)](../02_design_decisions.md)
