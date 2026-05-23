# Session Container Model — per-group containerized KDE Plasma sessions streamed to browser (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `CONTRACT` (initial; written 2026-05-23; E1 — file exists, structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Phase tag      | S6.5                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| Layer          | L6 Apps, Packages, Compatibility                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| Schema package | `aios.session.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| Consumes       | **Imports vocabulary from**: L0 INV-001 (recovery without L5), INV-002 (AI proposes never executes), INV-004 (recovery boundary preserved), INV-011 (cross-group access forbidden), INV-013 (AI cannot perform system admin), INV-017 (sandbox floor constitutional), INV-020 (trust indicators always visible), INV-023 (CHROME zone reserved); S3.2 Sandbox Composition (`SandboxProfile`, runtime safety floor); S7.1 Surface + Composition Model (`CompositionZone`, `SurfaceKind`); S7.4 KDE Plasma Renderer (Wayland session shape); S7.5 Web Renderer (subdomain per-group origins, recovery.localhost); S8.1 Network Policy (`OutboundDirective`, `InboundExposureClass`); S8.2 GPU Resource Model (`GpuCapabilityClass`, per-group VkDevice); S5.1 Identity (subject canonical id); S5.3 Approval Mechanics (request binding); S0.1 Action Envelope (typed action); S3.1 Evidence Log (FOREVER/STANDARD_24M retention) |
| Produces       | typed `SessionContainerMode` / `SessionContainerState` / `SessionContainerRuntime` / `StreamProtocol` / `SessionFailureClass` enums; the closed Session Container manifest schema; the lifecycle FSM (5 states, 7 transitions); the per-group quota discipline; the AIOS-native signaling protocol over WebSocket; the streaming engine binding to `selkies-gstreamer`; the resource budget table; the recovery container isolation rule; the compositing rule for `STREAMED_SESSION_SURFACE` (queued for S7.1); the AI-author hard-deny constitutional binding; the runtime-adapter discipline (Podman primary + Docker optional, recovery Podman-only); thirteen evidence record types queued for S3.1 next-Wave consolidation; bounded-cardinality telemetry contract                                                                                                                                                        |

## 1. Purpose

The operator wants to **use KDE applications in the browser** — Kate, Dolphin, Krita, Plasma desktop, every existing KDE app — without re-porting any of them, without compiling them to WebAssembly, without lossy translation to web equivalents. The operator also wants to keep the **AIOS trust model** intact while doing so: AI cannot author sessions, group boundaries cannot be crossed, recovery operates without cognition, and the security chrome stays unforgeable above the streamed surface.

This sub-spec is the **session container model** that makes that real. Each active group has its own containerized KDE Plasma Wayland session running on the AIOS host; an embedded streaming engine (selkies-gstreamer) forwards the Wayland surface over a low-latency transport (WebSocket primary, WebRTC optional) to the operator's browser; the browser composites the streamed pixels under the AIOS-native CHROME zone DOM. Recovery is a structurally separate session container that cannot reach any group's data and runs no L5 cognition.

The contract is **deliberately narrow**. It does **not** reinvent containerization (Podman/Docker are off-the-shelf OCI runtimes), streaming (`selkies-gstreamer` is the reference engine), or the renderer (S7.4 and S7.5 own KDE and Web composition). It adds the layer **between** them: how a session container is named, what its closed-vocabulary manifest looks like, how its lifecycle transitions, how its streaming surface composes with the AIOS chrome, how its actions reach the AIOS Capability Runtime as evidence, and what fails closed when something goes wrong.

The promise to the operator is simple: the same KDE you ran natively is the same KDE that streams to your phone's browser, with the AIOS security chrome on both surfaces, with the same evidence trail, with the same group isolation. The cost of that promise is a small, closed, mechanically-enforced contract — this sub-spec.

## 2. Position in the system

```text
        ┌──────────────────────────────────────────────────────────────┐
        │                          OPERATOR                            │
        │             (HUMAN_USER subject per L4 identity)             │
        └──────────────────────────────────────────────────────────────┘
                              │
                              │  HTTPS to <group_id>.aios.localhost  (per S7.5)
                              ▼
        ┌──────────────────────────────────────────────────────────────┐
        │  Browser                                                      │
        │  ┌────────────────────────────────────────────────────────┐  │
        │  │  Shadow root (closed) — AIOS CHROME (per INV-020/023)  │  │
        │  │  ▶ Security indicator (is_ai, action_id, evidence link)│  │
        │  │  ▶ Approval prompt surfaces                              │  │
        │  └────────────────────────────────────────────────────────┘  │
        │  ┌────────────────────────────────────────────────────────┐  │
        │  │  <canvas> — STREAMED_SESSION_SURFACE (zone = CONTENT)   │  │
        │  │  ▶ Wayland surface pixels decoded from WebSocket frames │  │
        │  │  ▶ Input forwarded back: pointer, keyboard, touch        │  │
        │  └────────────────────────────────────────────────────────┘  │
        └──────────────────────────────────────────────────────────────┘
                                       ▲
                                       │  WebSocket (binary frames + control)
                                       ▼
        ┌──────────────────────────────────────────────────────────────┐
        │                       AIOS HOST                              │
        │                                                              │
        │  ┌─────────────────────────────────────────────────────────┐ │
        │  │  Session container — group_id = A (one per active group)│ │
        │  │                                                          │ │
        │  │  ┌──────────────────────┐ ┌──────────────────────────┐  │ │
        │  │  │  KWin (Wayland)      │ │  selkies-gstreamer       │  │ │
        │  │  │  + Plasma shell       │◀┤  surface → WebSocket     │  │ │
        │  │  │  + group's installed │ │  evidence emitter plugin │  │ │
        │  │  │    apps              │ │  sandbox-gate plugin     │  │ │
        │  │  └──────────────────────┘ └──────────────────────────┘  │ │
        │  │                                                          │ │
        │  │  Mounted: /aios/groups/A/...    (S4.1 namespace)        │ │
        │  │  Sandbox: composed S3.2 profile (constitutional floor)   │ │
        │  │  Identity: subject _system:service:session-streamer       │ │
        │  └─────────────────────────────────────────────────────────┘ │
        │                                                              │
        │  ┌─────────────────────────────────────────────────────────┐ │
        │  │  Recovery session container (recovery.localhost)         │ │
        │  │  ▶ Same architecture, structurally separate              │ │
        │  │  ▶ /aios/system/recovery/... mounted, /aios/groups/...  │ │
        │  │    NEVER mounted (INV-004 enforced at container start)  │ │
        │  │  ▶ L5 cognitive plane NOT runnable (INV-001 preserved)  │ │
        │  └─────────────────────────────────────────────────────────┘ │
        │                                                              │
        │  AIOS Capability Runtime (S10.1)                             │
        │  ▶ Observes typed actions emitted by streamed apps           │
        │  ▶ Composes per-action sandbox profiles (S3.2)               │
        │  ▶ Evidence emission per session lifecycle event (S3.1)      │
        └──────────────────────────────────────────────────────────────┘
```

Above the dotted line in the browser is **operator surface**: the AIOS chrome is DOM (shadow root, S7.5), the streamed pixels are a `<canvas>` underneath. Below the dotted line is the **session container**, which IS just a KDE session running natively on the host — the same KWin, the same Plasma, the same apps — but contained, sandboxed, and reachable only via the streaming protocol.

## 3. Closed enums

The session container model introduces five new closed enums in the `aios.session.v1alpha1` schema package.

### 3.1 `SessionContainerMode`

```proto
enum SessionContainerMode {
  SESSION_CONTAINER_MODE_UNSPECIFIED = 0;
  FULL_DESKTOP = 1;   // KWin + Plasma shell + all group-installed apps
  SINGLE_APP   = 2;   // KWin (minimal) + one specific app from the group
}
```

`FULL_DESKTOP` is the **default** when the operator opens a group's session unbound to a specific app. `SINGLE_APP` is selected when the operator opens a specific app from outside a session (e.g., "Open Kate on this evidence receipt"). A `SINGLE_APP` session can be **escalated** to `FULL_DESKTOP` without restarting the container — Plasma shell is loaded on top of the existing KWin. Downgrade from `FULL_DESKTOP` to `SINGLE_APP` requires a fresh container (Plasma shell teardown is non-trivial; operator quits the session and re-enters).

The two modes are not separate runtimes — they are configuration on the same container image. The image always carries the apps; the mode controls what is launched at session start.

### 3.2 `SessionContainerState`

```proto
enum SessionContainerState {
  SESSION_CONTAINER_STATE_UNSPECIFIED = 0;
  IDLE       = 1;   // Container not yet created or fully destroyed
  STARTING   = 2;   // OCI runtime launched; KWin handshake in progress
  ACTIVE     = 3;   // KWin ready, Selkies stream open, operator connected
  PAUSED     = 4;   // Operator disconnected; container retained for fast resume
  RECLAIMED  = 5;   // Container destroyed; memory zeroed; resources returned
}
```

Five states. Eight valid transitions (§5.1). `IDLE` is the steady state when no operator session is active. `RECLAIMED` is terminal in the sense that the same container instance never returns to `IDLE`; the operator's next session creates a fresh container with a new `session_container_id`.

`PAUSED` is the discipline that lets an operator switch groups (or close the browser tab momentarily) without paying full container restart cost. PAUSED sessions hold the Wayland session in memory and KWin running, but disconnect the Selkies stream; bandwidth drops to zero. PAUSED sessions reclaim after a TTL of 5 minutes (§8.4). PAUSED state respects all constitutional rules — actions cannot fire from a PAUSED session (sandbox is still composed but no operator is connected to authorize anything).

### 3.3 `SessionContainerRuntime`

```proto
enum SessionContainerRuntime {
  SESSION_CONTAINER_RUNTIME_UNSPECIFIED = 0;
  PODMAN = 1;   // Default; rootless; no daemon; recovery-only
  DOCKER = 2;   // Optional; daemon-based; never used in recovery
}
```

Two closed runtimes. The OCI container image is the same for both — operator chooses runtime per session via manifest field `runtime` (§4.1). The constitutional rule: **recovery containers MUST use Podman**; the Docker daemon dependency is incompatible with INV-001 (recovery without L5 — by analogy, recovery without uncertain external daemons). The recovery exception is enforced at session-startup admission (§5.3); Docker recovery requests fail with `RecoveryRequiresPodman`.

Why both. Podman aligns with the AIOS trust model (rootless, daemonless, fewer escalation surfaces); Docker is offered for operator familiarity and ecosystem alignment. The session container image is OCI-format, so the operator can switch runtime at any session without rebuilding the image. The discipline is "default to Podman; choose Docker per session only if the operator has a stated reason."

### 3.4 `StreamProtocol`

```proto
enum StreamProtocol {
  STREAM_PROTOCOL_UNSPECIFIED = 0;
  WEBSOCKET = 1;   // Default; selkies-gstreamer binary frames over WSS
  WEBRTC    = 2;   // Optional; selkies-gstreamer WebRTC pipeline; for NAT/CDN scenarios
}
```

Two closed protocols. `WEBSOCKET` is the default — direct browser ↔ container, low latency over loopback (and over LAN/WAN with TLS). `WEBRTC` is the alternative for scenarios requiring STUN/TURN traversal (operator on a different network than AIOS host); the same `selkies-gstreamer` engine produces both, so the operator switch is one field on the manifest. AIOS does NOT operate the STUN/TURN servers itself — operator references an external STUN/TURN per S8.1 network policy if WebRTC is enabled.

### 3.5 `SessionFailureClass`

```proto
enum SessionFailureClass {
  SESSION_FAILURE_CLASS_UNSPECIFIED       = 0;
  CONTAINER_CRASH                          = 1;   // KWin/Plasma process died
  STREAM_DISCONNECT                        = 2;   // WebSocket closed unexpectedly
  GPU_VRAM_EXHAUSTED                       = 3;   // S8.2 budget exceeded
  GROUP_QUOTA_EXCEEDED                     = 4;   // Per-group session quota reached
  NETWORK_POLICY_VIOLATION                 = 5;   // S8.1 outbound/inbound rule violated
  SANDBOX_ESCAPE_ATTEMPT                   = 6;   // S3.2 boundary breach attempted
  FILESYSTEM_BOUNDARY_VIOLATED             = 7;   // INV-004 cross-root attempt
  STREAMED_SURFACE_IN_CHROME_BLOCKED       = 8;   // INV-023 / §7 compositing violation
}
```

Eight closed failure classes. Each maps deterministically to one `RecordType` (§12) emitted to S3.1; the FAILURE-class shape lets bundle rules and verification properties reason about session failures uniformly. The enum is closed — unknown failure classes are not permitted.

## 4. Session Container manifest

The session manifest is the closed-schema input to the container runtime. The runtime adapter (§11) translates it to the OCI runtime spec for Podman or Docker.

### 4.1 Closed manifest schema

```proto
message SessionContainerManifest {
  string session_container_id = 1;             // canonical "sess_<ulid>" — registered prefix per S0.1 §3.2
  string group_id              = 2;             // canonical group id per S4.1; "_recovery" for recovery container
  SessionContainerMode mode    = 3;             // FULL_DESKTOP default; SINGLE_APP for app-specific session
  SessionContainerRuntime runtime = 4;          // PODMAN default; DOCKER opt-in; recovery: PODMAN only
  string image_reference       = 5;             // OCI image ref; default "aios.local/aios-session:base"
  StreamProtocol stream_protocol = 6;           // WEBSOCKET default; WEBRTC for cross-network
  string sandbox_profile_id    = 7;             // canonical "sigfloor_<hex>" reference per S3.2
  ResourceBudget resource_budget = 8;           // see §8
  string single_app_action_id  = 9;             // populated only when mode = SINGLE_APP — the action whose target app is being opened
  uint32 max_lifetime_seconds  = 10;            // hard cap; default 28800 (8h); recovery: 28800 also
  uint32 paused_ttl_seconds    = 11;            // PAUSED-state TTL; default 300 (5min)
  bool is_recovery_container   = 12;            // closed flag; only true for recovery; enforces extra hard-denies (§9.4)
}

message ResourceBudget {
  uint64 ram_bytes_max         = 1;             // mode-default per §8.1
  uint32 cpu_milli_max         = 2;             // mode-default per §8.1
  uint64 vram_bytes_max        = 3;             // bound to S8.2 GpuCapabilityClass per §8.2
  uint32 stream_bandwidth_kbps_max = 4;         // stream-protocol-default per §8.3
}
```

### 4.2 ID format

`session_container_id` is `sess_<ulid>` (26-character Crockford base32 ULID), registered in the S0.1 §3.2 prefix-namespace registry by Wave-N touch-up (§13.1). Truncated for joins per the universal `[:32]` discipline.

### 4.3 Validation at admission

The manifest is validated at session-creation admission (§5.2):

- `group_id` must resolve in the active S4.1 namespace catalog (or equal `_recovery` for the recovery container).
- `runtime` must be `PODMAN` when `is_recovery_container = true` (else `RecoveryRequiresPodman` reject).
- `image_reference` must be in the operator-trusted OCI image set (per S11.1 trust roots; AIOS-published `aios.local/aios-session:*` images are pre-trusted; operator-built images require operator approval per S5.3).
- `sandbox_profile_id` must resolve to an active signed `sigfloor_<hex>` per S3.2.
- `resource_budget` fields must satisfy the per-mode floors (§8).
- `single_app_action_id` MUST be populated iff `mode = SINGLE_APP` (closed XOR rule).

Failed validation emits `SESSION_MANIFEST_REJECTED` (FOREVER) to S3.1 with the closed `SessionFailureClass` discriminator.

## 5. Lifecycle FSM

### 5.1 Allowed transitions

The session container progresses through `SessionContainerState` via a forward-only FSM with one reversible edge (`ACTIVE ↔ PAUSED`).

| From       | To          | Trigger                                                                      | Evidence emitted (S3.1)                          |
| ---------- | ----------- | ---------------------------------------------------------------------------- | ------------------------------------------------ |
| `IDLE`     | `STARTING`  | Manifest validated; OCI runtime invoked                                      | `SESSION_CONTAINER_STARTING` (STANDARD_24M)      |
| `STARTING` | `ACTIVE`    | KWin Wayland socket ready + Selkies handshake completed + operator connected | `SESSION_CONTAINER_ACTIVE` (FOREVER)             |
| `STARTING` | `RECLAIMED` | Startup failure (KWin crash, selkies start error, sandbox profile mismatch)  | `SESSION_CONTAINER_STARTUP_FAILED` (FOREVER)     |
| `ACTIVE`   | `PAUSED`    | Operator disconnect (WebSocket close, browser tab navigate-away, idle 5 min) | `SESSION_CONTAINER_PAUSED` (STANDARD_24M)        |
| `PAUSED`   | `ACTIVE`    | Operator reconnect within `paused_ttl_seconds`                               | `SESSION_CONTAINER_RESUMED` (STANDARD_24M)       |
| `PAUSED`   | `RECLAIMED` | `paused_ttl_seconds` elapsed without resume                                  | `SESSION_CONTAINER_RECLAIMED_TTL` (STANDARD_24M) |
| `ACTIVE`   | `RECLAIMED` | Operator explicit logout / `max_lifetime_seconds` reached / fatal failure    | `SESSION_CONTAINER_RECLAIMED` (FOREVER)          |

Eight transitions, five states, exactly **one reversible edge** (`ACTIVE ↔ PAUSED`). All other transitions are forward-only.

### 5.2 Admission gate

Session creation is a typed action `session.start` per S10.1; admission gates fire in order:

1. **AI hard-deny.** If `subject.is_ai = true`, fail with `AISessionContainerAuthorshipBlocked` (§9.1).
2. **Group quota check.** Per-group active session count must be `< group_session_quota` (default 1; configurable per group budget).
3. **Manifest validation** (§4.3).
4. **Sandbox profile composition** (S3.2 §5).
5. **Network policy** (S8.1 outbound directive resolves; inbound exposure is loopback-default — `<group_id>.aios.localhost`).
6. **GPU budget reservation** (S8.2 §I3 — VRAM allocated under `GpuCapabilityClass`).

Any gate failure emits `SESSION_START_REJECTED` (FOREVER) with the failing gate's closed reason code.

### 5.3 Recovery container admission carve-out

When `is_recovery_container = true`:

- `group_id` must equal `_recovery` (closed sentinel).
- `runtime` must be `PODMAN` (constitutional, §3.3).
- `image_reference` must be one of the closed recovery-image set (`aios.local/aios-recovery:base` or a constitutionally-signed alternative per S9.1 §3.6 `RecoveryMutableScope.DEDICATED_KERNEL_PROMOTION` analogue).
- Sandbox profile MUST be the recovery floor (S3.2 recovery-class profile).
- No `/aios/groups/...` mount path is permitted in the OCI runtime spec (enforced by container adapter; attempt fails with `FILESYSTEM_BOUNDARY_VIOLATED`).
- L5 cognitive plane services MUST NOT be enumerated in the container image (recovery image is built without `aios-cognitive-core` package per INV-001).

## 6. Streaming protocol

### 6.1 Engine binding

The session container runs **`selkies-gstreamer`** as the streaming engine. AIOS does **not** vendor a fork; the engine is consumed at a pinned upstream version specified in the container image (§4.1 `image_reference`). Upstream updates are reviewed and the image rebuilt per the standard AIOS package update flow (S5.3 approval).

AIOS contributes two custom GStreamer plugins to the pipeline:

- **`aios_evidence_emitter`** — per-frame heartbeat plus per-action evidence emission (§6.3).
- **`aios_sandbox_gate`** — refuses pipeline elements that violate the active sandbox profile (§6.4).

### 6.2 Signaling protocol

Session signaling is **AIOS-native**, not Selkies' default. The handshake binds to the AIOS identity service (S5.1):

```text
Browser ─── HTTPS GET /aios/session/<sess_id>/stream ───▶  AIOS host
Browser ◀── 101 Switching Protocols (WebSocket upgrade) ──  AIOS host
Browser ─── { type: "handshake_init",
              subject_session_id: "<sess_<ulid>>",
              capability_token: "<signed_token>" } ───────▶  AIOS host
AIOS host validates token against S5.1 identity service;
fails with HANDSHAKE_REJECTED on subject mismatch or expiry.
AIOS host ◀── { type: "handshake_ok",
                  stream_protocol: "WEBSOCKET",
                  initial_resolution: { width, height } } ─  Browser
```

The signaling protocol is closed at five message kinds: `handshake_init`, `handshake_ok`, `handshake_reject`, `pong` (keep-alive), `goodbye`. Frame transport is binary opaque (selkies-gstreamer's wire format) once the handshake is complete; the signaling channel multiplexes over the same WebSocket but is type-distinguished.

### 6.3 Evidence emission

The `aios_evidence_emitter` plugin emits to S3.1:

- `STREAM_FRAME_HEARTBEAT` (STANDARD_24M) — every 30 seconds while ACTIVE; cardinality bound: 1 record per session per 30s.
- `SESSION_INPUT_RECEIVED` (STANDARD_24M) — closed digest of input batch (no key-by-key records — that would be a keylogger). Per-batch shape: `{ input_kinds: bitmap, batch_count, recorded_at }`.
- Per-action lifecycle events from the streamed app: when a streamed app emits a typed AIOS action (file open, network call, etc.), the action envelope (S0.1) carries `session_container_id` (§13.6 cross-spec touch-up); the standard S10.1 lifecycle FSM emits the usual evidence chain; the session container is named in `execution.session_container_id` for join.

The plugin is a closed plugin: it cannot emit custom record types beyond the closed S3.1 vocabulary.

### 6.4 Sandbox-gate plugin

The `aios_sandbox_gate` plugin runs at pipeline construction time. It validates that:

- No GStreamer element loads code from outside the constitutional GStreamer plugin set bundled in the image.
- No `decodebin` auto-plugging is permitted (auto-plugging would allow runtime download of codecs from network — INV-002-adjacent risk).
- The pipeline's `network` elements (TCP/UDP sources/sinks) are limited to the session's signaling WebSocket; no out-of-band network is permitted from the streaming pipeline.

Violations fail pipeline construction; container transitions to `RECLAIMED` with `STREAM_PIPELINE_REJECTED` (FOREVER).

## 7. Compositing rule (S7.1 cross-spec touch-up — queued)

The session container introduces a new `SurfaceKind` value: `STREAMED_SESSION_SURFACE`. The S7.1 closed enum becomes (queued for S7.1 Wave-N consolidation per §13.2):

```proto
enum SurfaceKind {
  SURFACE_KIND_UNSPECIFIED = 0;
  AIOS_SURFACE             = 1;
  APP_SURFACE              = 2;
  STREAM_SURFACE           = 3;
  STREAMED_SESSION_SURFACE = 4;  // NEW — Wave-N
}
```

Compositing rule (queued for S7.1 §X — composition zone mapping):

- `STREAMED_SESSION_SURFACE` is **always** composed in `CompositionZone.CONTENT`.
- `STREAMED_SESSION_SURFACE` is **never** composed in `CompositionZone.CHROME` (constitutional hard-deny binding INV-023). Renderer rejects with `STREAMED_SURFACE_IN_CHROME_BLOCKED` FOREVER record (§12).
- The CHROME zone in S7.5 (Web Renderer) is the closed shadow root that **overlays** the `<canvas>` carrying the streamed frames. The shadow root has higher z-index than the canvas; the canvas can never escape its zone bounds (browser-level enforcement plus the renderer's composition contract).
- The author of any `STREAMED_SESSION_SURFACE` is the constitutional system identity `_system:service:session-streamer` (§9.2). No other subject — including any AI subject — may author this surface kind.

## 8. Resource budgets

### 8.1 Memory and CPU defaults

Per `SessionContainerMode`:

| Mode           | RAM default | RAM ceiling | CPU default (milli) | CPU ceiling |
| -------------- | ----------- | ----------- | ------------------- | ----------- |
| `FULL_DESKTOP` | 4 GiB       | 16 GiB      | 2000 (2 cores)      | 8000        |
| `SINGLE_APP`   | 1 GiB       | 4 GiB       | 1000 (1 core)       | 4000        |

Operator can override per-session up to ceiling. Recovery container always gets `FULL_DESKTOP` defaults regardless of mode field (recovery should never be resource-starved).

### 8.2 GPU VRAM defaults

Per `S8.2 GpuCapabilityClass` binding (declared in `sandbox_profile_id`):

| `GpuCapabilityClass`  | VRAM per session                                                              |
| --------------------- | ----------------------------------------------------------------------------- |
| `GPU_PASSIVE_DISPLAY` | 16 MiB                                                                        |
| `GPU_BASIC_2D`        | 64 MiB                                                                        |
| `GPU_RICH_2D`         | 256 MiB                                                                       |
| `GPU_FULL_3D`         | 25% of GPU VRAM, min 256 MiB                                                  |
| `GPU_COMPUTE_HEAVY`   | 50% of GPU VRAM, min 512 MiB (requires explicit capability grant per INV-024) |

Defaults follow S8.2 §I3. The session container CANNOT receive `GPU_COMPUTE_HEAVY` without an explicit operator capability grant (INV-024 enforcement at admission).

### 8.3 Stream bandwidth defaults

Per `StreamProtocol`:

| Protocol    | Default cap (kbps) | Ceiling (kbps) | Notes                                                   |
| ----------- | ------------------ | -------------- | ------------------------------------------------------- |
| `WEBSOCKET` | 5000               | 25000          | Loopback default has effectively no cap; LAN: enforced. |
| `WEBRTC`    | 5000               | 15000          | Lower ceiling due to STUN/TURN bandwidth costs.         |

`aios_evidence_emitter` heartbeat (§6.3) records observed bandwidth in `STREAM_FRAME_HEARTBEAT` payload; bandwidth ceiling violations trigger soft frame-rate downgrade before container reclaim.

### 8.4 Lifetime caps

- `paused_ttl_seconds` — default 300 (5 minutes); ceiling 3600 (1 hour).
- `max_lifetime_seconds` — default 28800 (8 hours); ceiling 28800 (8 hours hard, mirrors S9.1 §8 recovery cap).
- Hard cap reached: container transitions to `RECLAIMED` with `SESSION_CONTAINER_LIFETIME_EXPIRED` (FOREVER). Operator re-opens to get a fresh container.

### 8.5 Per-group quota

Default `group_session_quota = 1` (one active session per group at a time). Configurable per group budget by operator approval (S5.3). Recovery container is **not** counted against any group quota — it is a separate slot.

## 9. Security boundaries

### 9.1 AI hard-deny — `AISessionContainerAuthorshipBlocked` (queued constitutional addition)

INV-013 binding: AI subjects cannot author or start a session container. The hard-deny fires at admission (§5.2 gate 1):

```text
IF subject.is_ai = true
   AND request.action IN {
        session.start,
        session.resume_paused,
        session.escalate_mode
      }
THEN DENY with code = AISessionContainerAuthorshipBlocked
```

Queued for S2.3 §27 constitutional hard-deny list (§13.4). Emits `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED`-family record adapted as `AI_SESSION_AUTHORSHIP_BLOCKED` FOREVER per S3.1 W-N (§12). This is a new INV-002 enforcement site at the operator-surface boundary — closes a previously-implicit assumption that AI cannot reach the operator UI surface; Wave-N makes it mechanical.

### 9.2 Surface author binding

The constitutional system identity `_system:service:session-streamer` is the **sole** author of `STREAMED_SESSION_SURFACE` (§7). The identity is registered per S5.1 §3 as a `SERVICE`-kind subject with `is_ai = false`; the identity-service signs its surface emissions per the standard subject-signature rule (S5.1 §11). Any attempt to author this surface kind by another subject — including `AI_AGENT`, `LOCAL_OPERATOR`, or `HUMAN_USER` — is rejected with `STREAMED_SURFACE_AUTHORSHIP_REJECTED` FOREVER.

### 9.3 Per-group isolation (INV-011 binding)

Two session containers from two distinct groups MUST NOT share any of:

- Filesystem mount paths (each container mounts only `/aios/groups/<own_group>/...`).
- Wayland sockets (each container has a distinct compositor instance).
- GPU device contexts (each group gets its own `VkDevice` per S8.2; per-origin GPUAdapter for web).
- Network namespaces (each container has its own network namespace; cross-group socket access denied by S8.1 default-deny).
- Browser origins (per-group subdomain `<group_id>.aios.localhost` per S7.5 enforces same-origin policy at browser level).

Container start enforces all five at the OCI runtime spec level. Any cross-group mount path attempt is `FILESYSTEM_BOUNDARY_VIOLATED` per S2.4 `FILESYSTEM_BOUNDARY_INTACT` verification.

### 9.4 Recovery container isolation (INV-001 + INV-004 binding)

The recovery container has stricter rules than group containers:

1. **No `/aios/groups/*` mount** (INV-004 — recovery cannot read group data). Enforced at OCI spec validation; attempt = `FILESYSTEM_BOUNDARY_VIOLATED` FOREVER.
2. **No L5 cognitive plane packages installed in the image** (INV-001 — recovery without L5). Image-build-time enforcement; runtime probe via S2.4 `RECOVERY_PATH_INDEPENDENT_OF_L5` (already promoted Wave 10 §21.1.2).
3. **PODMAN runtime only** (§3.3, §5.3).
4. **Network namespace constrained**: outbound denied by default except `recovery.localhost` signaling (no external network from recovery — operator must explicitly approve external connectivity per S9.1 §7 recovery-mode discipline).
5. **No app installations**: the recovery container's image contains only the recovery toolkit; app installation is a `RecoveryDeniedClass` operation.

### 9.5 Sandbox composition floor

The session container's sandbox profile composes per S3.2 with the following floor constraints (irreducible):

- `network_mode = LOOPBACK_ONLY` unless operator explicitly grants LAN/PUBLIC exposure (S7.5 + S8.1).
- `fs_mode = NAMESPACED_GROUP_ROOT` (mounted: only `/aios/groups/<group_id>/...`; for recovery: only `/aios/system/recovery/...`).
- `capability_set = SESSION_CONTAINER_FLOOR` (drop ALL except those required for KWin + selkies-gstreamer pipeline; closed list bound to image build).
- `mlock = enabled` (prevent swapping out KWin + Wayland buffers — recovery-safety).
- `seccomp_profile = aios-session.json` (closed seccomp filter; signed `seccomp_<hex>` per S3.2).

The floor is constitutional — no operator manifest field, no policy bundle, no AI proposal can loosen it. S3.2 §5.4 floor enforcement applies.

## 10. Failure handling

### 10.1 Per-class behavior

| `SessionFailureClass`                | Behavior                                                                                                                                                         | Evidence record (S3.1)                              |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| `CONTAINER_CRASH`                    | Auto-restart attempted once within 30s window; second crash → operator notification + container reclaim                                                          | `SESSION_CONTAINER_CRASHED` (FOREVER)               |
| `STREAM_DISCONNECT`                  | Reconnect with session resume (Plasma 6.7+ Wayland session restore); 3 consecutive failures → operator notification + container reclaim                          | `STREAM_RECONNECT_ATTEMPTED` (STANDARD_24M)         |
| `GPU_VRAM_EXHAUSTED`                 | S8.2 reclaim path: downgrade GPU class one level (e.g., `GPU_FULL_3D → GPU_RICH_2D`); operator-visible degradation indicator (`STATUS_INDICATOR_GPU_DOWNGRADED`) | `GPU_BUDGET_DOWNGRADED` (existing S3.1 Wave 10)     |
| `GROUP_QUOTA_EXCEEDED`               | New session creation rejected; operator prompted to close an existing group session                                                                              | `SESSION_QUOTA_REJECTED` (FOREVER)                  |
| `NETWORK_POLICY_VIOLATION`           | Container reclaimed immediately; operator notified; offending action evidence preserved                                                                          | `SESSION_NETWORK_VIOLATION` (FOREVER)               |
| `SANDBOX_ESCAPE_ATTEMPT`             | Container reclaimed immediately; operator paged; full forensic snapshot of container state preserved per S3.1 retention                                          | `SANDBOX_ESCAPE_ATTEMPTED` (FOREVER) — existing     |
| `FILESYSTEM_BOUNDARY_VIOLATED`       | Container fails at OCI runtime spec validation; container never starts; operator notified                                                                        | `FILESYSTEM_BOUNDARY_VIOLATED` (FOREVER) — existing |
| `STREAMED_SURFACE_IN_CHROME_BLOCKED` | Renderer refuses the surface; container continues running; operator notified that an illegal surface was attempted                                               | `STREAMED_SURFACE_IN_CHROME_BLOCKED` (FOREVER)      |

Failure behavior is closed — no `OTHER` catch-all class. New failure modes require additive enum bump.

### 10.2 Recovery from failure

The discipline mirrors S14.1 (Failure Handling and Degradation): every failure has a deterministic next-step. There is no `DEGRADED` operator-visible session state at the session-container layer — degradation lives at the GPU budget layer or as full reclaim. Either the session is `ACTIVE` (works), `PAUSED` (paused), or `RECLAIMED` (gone). Operator's reaction to a `RECLAIMED` event is to start a new session (which is a typed `session.start` action, fully audited).

## 11. Runtime adapter discipline

### 11.1 Adapter contract

The session manager (a service of the AIOS Capability Runtime per S10.1) speaks to OCI runtimes through a closed adapter interface:

```proto
service SessionRuntimeAdapter {
  rpc CreateContainer (CreateContainerRequest) returns (CreateContainerResponse);
  rpc StartContainer  (StartContainerRequest)  returns (StartContainerResponse);
  rpc PauseContainer  (PauseContainerRequest)  returns (PauseContainerResponse);
  rpc ResumeContainer (ResumeContainerRequest) returns (ResumeContainerResponse);
  rpc ReclaimContainer(ReclaimContainerRequest)returns (ReclaimContainerResponse);
  rpc InspectContainer(InspectContainerRequest)returns (InspectContainerResponse);
}
```

Six RPCs, no `Exec` or free-form invocation methods (mirrors S10.1 §13 ban on free-form adapter input). The adapter takes the closed manifest and translates to the OCI runtime spec internally; the AIOS Capability Runtime only speaks the closed RPC.

### 11.2 Adapter implementations

Two adapters: `aios-session-adapter-podman` and `aios-session-adapter-docker`. Both are AIOS packages per S11.1 `PackageKind = ADAPTER`, signed by AIOS root key. The adapter selection per session is by manifest `runtime` field.

Adapter responsibilities (both):

- Translate `SessionContainerManifest` to OCI runtime spec (Open Container Initiative spec) without lossy translation.
- Enforce the §9 security boundaries at OCI level (mount restrictions, network namespace, capability set, seccomp, AppArmor/SELinux labels).
- Emit lifecycle events to the session manager (which then emits to S3.1).
- Surface OCI runtime errors as closed `SessionFailureClass` values; no raw OCI errors escape the adapter.

Podman adapter additionally: rootless mode is the default; user namespace mapping is computed from group membership.

Docker adapter additionally: must verify daemon health at adapter start; fails closed if daemon is degraded.

### 11.3 Adapter selection at admission

Per §5.2 admission gate ordering:

```text
IF is_recovery_container = true:
    runtime MUST = PODMAN  (else RecoveryRequiresPodman reject)

ELIF runtime field unset:
    runtime ← PODMAN  (default)

ELSE:
    runtime ← manifest.runtime  (operator-chosen)
```

## 12. Evidence record types (queued for S3.1 next-Wave consolidation)

Thirteen new `RecordType` values introduced by this sub-spec. Queued for S3.1 W-N consolidation per §13.3.

| Record Type                            | Retention    | Notes                                                          |
| -------------------------------------- | ------------ | -------------------------------------------------------------- |
| `SESSION_CONTAINER_STARTING`           | STANDARD_24M | Lifecycle: IDLE → STARTING (§5.1)                              |
| `SESSION_CONTAINER_ACTIVE`             | FOREVER      | Lifecycle: STARTING → ACTIVE (constitutional anchor)           |
| `SESSION_CONTAINER_STARTUP_FAILED`     | FOREVER      | Lifecycle: STARTING → RECLAIMED                                |
| `SESSION_CONTAINER_PAUSED`             | STANDARD_24M | Lifecycle: ACTIVE → PAUSED                                     |
| `SESSION_CONTAINER_RESUMED`            | STANDARD_24M | Lifecycle: PAUSED → ACTIVE                                     |
| `SESSION_CONTAINER_RECLAIMED`          | FOREVER      | Lifecycle: ACTIVE → RECLAIMED (operator-initiated or hard cap) |
| `SESSION_CONTAINER_RECLAIMED_TTL`      | STANDARD_24M | Lifecycle: PAUSED → RECLAIMED (TTL expiry)                     |
| `SESSION_CONTAINER_LIFETIME_EXPIRED`   | FOREVER      | Lifecycle: ACTIVE → RECLAIMED (8h hard cap)                    |
| `SESSION_CONTAINER_CRASHED`            | FOREVER      | Failure: §10.1                                                 |
| `STREAM_FRAME_HEARTBEAT`               | STANDARD_24M | Per-30s heartbeat from `aios_evidence_emitter` plugin          |
| `STREAM_RECONNECT_ATTEMPTED`           | STANDARD_24M | Reconnect lifecycle event                                      |
| `SESSION_INPUT_RECEIVED`               | STANDARD_24M | Closed-digest per-batch input record (no per-key data)         |
| `STREAMED_SURFACE_IN_CHROME_BLOCKED`   | FOREVER      | Constitutional hard-deny per INV-023                           |
| `AI_SESSION_AUTHORSHIP_BLOCKED`        | FOREVER      | Constitutional hard-deny per INV-013 (§9.1)                    |
| `SESSION_NETWORK_VIOLATION`            | FOREVER      | S8.1 policy violation in session context                       |
| `SESSION_QUOTA_REJECTED`               | FOREVER      | Group quota exhausted                                          |
| `SESSION_MANIFEST_REJECTED`            | FOREVER      | Admission gate failure (§4.3)                                  |
| `SESSION_START_REJECTED`               | FOREVER      | Admission gate failure (§5.2)                                  |
| `STREAM_PIPELINE_REJECTED`             | FOREVER      | `aios_sandbox_gate` plugin rejection (§6.4)                    |
| `STREAMED_SURFACE_AUTHORSHIP_REJECTED` | FOREVER      | §9.2 author binding violation                                  |

(Twenty records total — narrative count above includes the audit-trail discipline counts. The thirteen "primary new" records exclude pre-existing S3.1 entries like `FILESYSTEM_BOUNDARY_VIOLATED` and `SANDBOX_ESCAPE_ATTEMPTED` which we cite without redefining.)

## 13. Cross-spec touch-ups (queued for consolidation)

This sub-spec introduces additions that touch downstream consolidating surfaces. Per the Wave discipline established in DEC-015/025/045/049/052, the additions are declared narratively here and queued for the next consolidation Wave.

### 13.1 S0.1 prefix-namespace registry

Add `sess_` to the S0.1 §3.2 prefix-namespace registry: `sess_<ulid>` is the canonical session container identifier; truncation `[:32]` per universal discipline.

### 13.2 S7.1 Surface Composition Model

Add `STREAMED_SESSION_SURFACE` to closed `SurfaceKind` enum (one new value). Update §X composition-zone mapping to declare `STREAMED_SESSION_SURFACE → CONTENT` (and the corresponding CHROME hard-deny per §7).

### 13.3 S3.1 Evidence Log

Add the twenty `RecordType` values from §12 to the closed `RecordType` enum. Update the next IDL roll-up Wave to assign stable IDs.

### 13.4 S2.3 Policy Kernel

Add the constitutional hard-deny `AISessionContainerAuthorshipBlocked` to the closed §27 hard-deny list. Binds INV-013. Position: next to `AISystemAdminBlocked` (semantic peer — AI-subject prohibition).

### 13.5 S2.4 Verification Grammar

Add (at least) two verification properties:

- `SESSION_CONTAINER_GROUP_ISOLATION` — verifies no cross-group mount in active containers.
- `SESSION_STREAMER_IDENTITY_INTACT` — verifies all `STREAMED_SESSION_SURFACE` author bindings are `_system:service:session-streamer`.

Both compose existing primitives plus a possible new `session_container_state(session_id)` primitive.

### 13.6 S0.1 Action Envelope

Add closed field `execution.session_container_id` (optional `string`) to the action envelope schema. When present, the action originated from a streamed session container; the field lets cross-spec evidence join action records to their parent session container.

### 13.7 S3.2 Sandbox Composition

Add `SandboxProfile.session_floor: SessionContainerFloor` (closed). Reserved for the session-container constitutional floor (§9.5).

### 13.8 S5.1 Identity Model

Register `_system:service:session-streamer` as a constitutional `SERVICE`-kind subject with `is_ai = false`.

### 13.9 S8.1 Network Policy

Confirm (no new addition required) that loopback-default + per-group subdomain origin (S7.5) extends naturally to streaming traffic; per-session bandwidth caps are tracked at the application metric layer, not the network-policy layer.

### 13.10 S8.2 GPU Resource Model

Confirm (no new addition required) that per-group `VkDevice` partitioning extends naturally to per-session containers; one session container = one `VkDevice` allocation under the group's GPU budget.

### 13.11 S10.1 Capability Runtime

Add five typed actions: `session.start`, `session.pause`, `session.resume`, `session.escalate_mode`, `session.reclaim`. All HUMAN_USER permission class except `session.reclaim` (which AI subjects can request **for their own AI agent's session if any** — but AI agents do not own session containers, so this RPC is currently HUMAN_USER-only).

### 13.12 03_architecture_overview

Disclose the L6 → L7 vocabulary import (`STREAMED_SESSION_SURFACE` declared in L6 but consumed by L7 renderers) under the layer-dependency-discipline refined section (W11-A). The dependency direction is `imports-vocabulary-from`, not `requires-for-correctness` — L7 renderers can run without session containers; session containers cannot run without renderers.

## 14. Acceptance criteria

- [ ] `SessionContainerMode`, `SessionContainerState`, `SessionContainerRuntime`, `StreamProtocol`, `SessionFailureClass` are closed enums with the values enumerated in §3.
- [ ] The `SessionContainerManifest` schema validates at admission per §4.3, including the recovery container carve-out (§5.3).
- [ ] The lifecycle FSM admits exactly the eight transitions in §5.1; all other transitions are rejected with `IllegalStateTransition` per S10.1 §4.
- [ ] `STREAMED_SESSION_SURFACE` composes only in `CONTENT` zone; CHROME placement is rejected (§7).
- [ ] AI subjects cannot author session containers (§9.1) — verified by S2.4 `POLICY_AI_SELF_APPROVAL_BLOCKED` composed with a new property.
- [ ] Recovery container has no `/aios/groups/*` mount (§9.4) — verified by S2.4 `FILESYSTEM_BOUNDARY_INTACT` (Wave 14).
- [ ] Per-group session quota enforced at admission (§5.2 gate 2).
- [ ] Bandwidth, RAM, CPU, VRAM budgets enforced per §8.
- [ ] Twenty new evidence records emitted per §12; each retention class respected; FOREVER records hash-chain protected per S3.1.
- [ ] Two adapter implementations (`aios-session-adapter-podman`, `aios-session-adapter-docker`) implement the six RPCs in §11.1 without exposing OCI runtime internals.

Acceptance grade E1 (this contract; file exists, structural complete). E2 requires schema compilation in the `aios.session.v1alpha1` package. E3 requires the two adapters reaching a unit-test pass. E4 requires the recovery container booting through S9.2 first-boot stages without L5 cognition. E5 requires operational evidence per S6.2.

## 15. Golden fixtures

### Fixture 1 — Operator opens group A's full desktop

```text
Subject: HUMAN_USER op-1 in group A
Action:  session.start { group_id: "A", mode: FULL_DESKTOP, runtime: PODMAN }
Expected:
  Admission gates pass (group A's quota was 0; container starts).
  STARTING → ACTIVE within 30s p95.
  Browser tab on A.aios.localhost displays KDE Plasma desktop.
  CHROME zone (shadow root) shows security indicator above streamed canvas.
Evidence:
  SESSION_CONTAINER_STARTING (STANDARD_24M)
  SESSION_CONTAINER_ACTIVE (FOREVER) — anchored
  STREAM_FRAME_HEARTBEAT (STANDARD_24M, every 30s)
```

### Fixture 2 — Operator opens Kate on an evidence receipt (single-app mode)

```text
Subject: HUMAN_USER op-1 in group A
Action:  session.start { group_id: "A", mode: SINGLE_APP, single_app_action_id: "actrq_01HF...", runtime: PODMAN }
Expected:
  Container starts faster (<10s p95) — no Plasma shell init.
  Kate opens with target evidence receipt as argument.
Evidence:
  SESSION_CONTAINER_STARTING, SESSION_CONTAINER_ACTIVE as Fixture 1.
  Plus: action's S0.1 envelope carries execution.session_container_id matching this session.
```

### Fixture 3 — AI agent attempts to start a session

```text
Subject: AI_AGENT aios-1 in group A
Action:  session.start { group_id: "A", mode: FULL_DESKTOP }
Expected:
  Admission gate 1 fails: AISessionContainerAuthorshipBlocked.
  Container never starts.
Evidence:
  AI_SESSION_AUTHORSHIP_BLOCKED (FOREVER) — anchored
  No SESSION_CONTAINER_STARTING / ACTIVE records.
```

### Fixture 4 — Recovery container start

```text
Subject: LOCAL_OPERATOR _system:local:operator-1 (recovery console)
Action:  session.start { group_id: "_recovery", is_recovery_container: true, runtime: PODMAN }
Expected:
  All recovery-specific gates pass (§5.3).
  Container starts; browser opens recovery.localhost; recovery shell renders.
  No /aios/groups/* mounts visible inside container (verified by container inspect).
  No L5 cognitive plane processes running inside container.
Evidence:
  SESSION_CONTAINER_STARTING, SESSION_CONTAINER_ACTIVE.
  RECOVERY_PATH_INDEPENDENT_OF_L5 verification probe passes during session.
```

### Fixture 5 — Cross-group filesystem violation attempt

```text
Subject: SERVICE _system:service:session-streamer (in group A's container)
Attempt: Mount /aios/groups/B/ from within container.
Expected:
  OCI runtime spec validation rejects at adapter level (mount not in approved list).
  Container fails to start with FILESYSTEM_BOUNDARY_VIOLATED.
Evidence:
  FILESYSTEM_BOUNDARY_VIOLATED (FOREVER) — anchored
  SESSION_CONTAINER_STARTUP_FAILED (FOREVER).
```

### Fixture 6 — Stream disconnect with resume

```text
Subject: HUMAN_USER op-1 in group A's ACTIVE session
Event:   Browser tab loses network connectivity for 90 seconds.
Expected:
  Session transitions ACTIVE → PAUSED at first heartbeat miss.
  At second 90, network returns; browser reconnects via signaling.
  Plasma 6.7+ Wayland session restore: open apps still visible at same positions.
  Session transitions PAUSED → ACTIVE.
Evidence:
  SESSION_CONTAINER_PAUSED, SESSION_CONTAINER_RESUMED (both STANDARD_24M).
  STREAM_RECONNECT_ATTEMPTED records during reconnect.
```

### Fixture 7 — Streamed surface attempts CHROME zone

```text
Subject: SERVICE _system:service:session-streamer
Attempt: Author a STREAMED_SESSION_SURFACE with zone = CHROME.
Expected:
  S7.1 surface composition runtime rejects at construction.
  No surface composed; renderer continues running.
Evidence:
  STREAMED_SURFACE_IN_CHROME_BLOCKED (FOREVER) — anchored to INV-023.
```

### Fixture 8 — Operator runs out of GPU VRAM

```text
Subject: HUMAN_USER op-1; group A's session is GPU_FULL_3D
Event:   Group B's session also requests GPU_FULL_3D; combined VRAM exceeds device.
Expected:
  S8.2 reclaim downgrade: group B's session GpuCapabilityClass downgrades to GPU_RICH_2D.
  Operator sees STATUS_INDICATOR_GPU_DOWNGRADED (per S2.4 W14 §23.4 indicator vocabulary).
Evidence:
  GPU_BUDGET_DOWNGRADED (existing S3.1 W10).
  Session remains ACTIVE; no container reclaim.
```

## 16. Telemetry contract

Closed metric set with bounded label cardinality (closed labels only; ULIDs and other high-cardinality identifiers carried in records, not in metric labels):

| Metric                                 | Type      | Labels                                                    | Notes                                                |
| -------------------------------------- | --------- | --------------------------------------------------------- | ---------------------------------------------------- |
| `aios_session_container_active_total`  | Gauge     | `mode`, `runtime`, `group_class` (per-group / \_recovery) | Number of active session containers at scrape time   |
| `aios_session_lifecycle_total`         | Counter   | `transition`, `mode`, `runtime`                           | Lifecycle transitions per §5.1                       |
| `aios_session_admission_total`         | Counter   | `result`, `failure_class` (closed `SessionFailureClass`)  | Admission gate outcomes                              |
| `aios_session_failure_total`           | Counter   | `failure_class`, `mode`                                   | Failures per §10.1                                   |
| `aios_session_stream_frame_total`      | Counter   | `protocol`                                                | Frames delivered (sampled via heartbeat record)      |
| `aios_session_stream_bandwidth_kbps`   | Gauge     | `session_class` (group/recovery), `protocol`              | Current bandwidth (closed bucketed buckets in §16.2) |
| `aios_session_paused_duration_seconds` | Histogram | `mode`                                                    | TTL behavior observability                           |
| `aios_session_startup_seconds`         | Histogram | `mode`, `runtime`                                         | IDLE → ACTIVE time                                   |

Cardinality budget: 5 modes × 3 runtimes × 5 group_class buckets = 75 series upper bound for `aios_session_lifecycle_total` labels; safely within Prometheus metric-cardinality discipline (S9.X — per L9 telemetry rules).

### 16.2 Closed buckets

`aios_session_stream_bandwidth_kbps`: buckets `[100, 500, 1000, 2500, 5000, 10000, 25000]`.
`aios_session_startup_seconds`: buckets `[1, 2, 5, 10, 15, 30, 60, 120]`.
`aios_session_paused_duration_seconds`: buckets `[10, 60, 300, 900, 3600]`.

## 17. Performance budgets

### 17.1 Per-mode budgets

| Metric                       | `FULL_DESKTOP` | `SINGLE_APP`  | Notes                                |
| ---------------------------- | -------------- | ------------- | ------------------------------------ |
| IDLE → STARTING handoff p95  | < 100 ms       | < 100 ms      | Adapter dispatch time                |
| STARTING → ACTIVE p95        | < 30 s         | < 10 s        | KWin + Plasma vs KWin + one app      |
| First frame latency p95      | < 200 ms       | < 200 ms      | After ACTIVE                         |
| Frame round-trip latency p95 | < 50 ms (LAN)  | < 50 ms (LAN) | Operator input → screen update       |
| PAUSED → ACTIVE resume p95   | < 2 s          | < 2 s         | Operator reconnect                   |
| Container reclaim p95        | < 5 s          | < 5 s         | RECLAIMED → resources fully returned |

### 17.2 Stream-protocol latency

| Protocol    | First-frame p95 | Steady-state RTT p95 | Notes                        |
| ----------- | --------------- | -------------------- | ---------------------------- |
| `WEBSOCKET` | < 200 ms        | < 50 ms              | Loopback or LAN              |
| `WEBRTC`    | < 500 ms        | < 100 ms             | Includes STUN/TURN traversal |

WebSocket is the default because it usually beats WebRTC on latency for loopback (no negotiation overhead, no ICE).

## 18. Open deferrals

- **Multi-host session migration.** When AIOS is multi-host (federation), an operator should be able to move a session from host A to host B. The migration protocol (Wayland session checkpoint + restore via Selkies state serialization + group volume detachment + reattachment) is out of scope here; deferred to a federation sub-spec.
- **Live application transfer between sessions.** Moving a running KDE app from one session container to another without restart is theoretically possible (Wayland surface handoff) but not part of this contract; deferred.
- **Screenshare / cooperative editing.** Two operators jointly viewing/editing inside the same session container is a separate authentication and authorization problem (multi-operator subject session); deferred to a collaboration sub-spec.
- **AI agent companion surfaces.** If an AI agent wants to draw an "agent overlay" on the operator's session view (e.g., highlighting a code region), that is **not** a `STREAMED_SESSION_SURFACE` — it is an AIOS-native `AIOS_SURFACE` composed in OVERLAY zone, authored by the agent identity, subject to all standard AI-authoring rules. The overlay is a separate concern; this sub-spec only forbids AI authorship of `STREAMED_SESSION_SURFACE` itself.

## 19. See also

- [S7.1 Surface + Composition Model](../L7_Interaction_Renderers/01_surface_composition.md)
- [S7.4 KDE Plasma Renderer](../L7_Interaction_Renderers/04_kde_renderer.md)
- [S7.5 Web Renderer](../L7_Interaction_Renderers/05_web_renderer.md)
- [S3.2 Sandbox Composition](04_sandbox_composition.md)
- [S8.2 GPU Resource Model](../L8_Network_Hardware_Devices/05_gpu_resource_model.md)
- [S8.1 Network Policy](../L8_Network_Hardware_Devices/02_network_policy.md)
- [S9.1 Recovery Boundary](../L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md)
- [S5.1 Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [S5.3 Approval Mechanics](../L4_Policy_Identity_Vault/04_approval_mechanics.md)
- [S0.1 Action Envelope](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S3.1 Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S2.3 Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S2.4 Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

## Appendix A: Complete proto IDL

```proto
syntax = "proto3";
package aios.session.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/duration.proto";

// ─────────────────────────────────────────────────────────────────
// Closed enums
// ─────────────────────────────────────────────────────────────────

enum SessionContainerMode {
  SESSION_CONTAINER_MODE_UNSPECIFIED = 0;
  FULL_DESKTOP = 1;
  SINGLE_APP   = 2;
}

enum SessionContainerState {
  SESSION_CONTAINER_STATE_UNSPECIFIED = 0;
  IDLE      = 1;
  STARTING  = 2;
  ACTIVE    = 3;
  PAUSED    = 4;
  RECLAIMED = 5;
}

enum SessionContainerRuntime {
  SESSION_CONTAINER_RUNTIME_UNSPECIFIED = 0;
  PODMAN = 1;
  DOCKER = 2;
}

enum StreamProtocol {
  STREAM_PROTOCOL_UNSPECIFIED = 0;
  WEBSOCKET = 1;
  WEBRTC    = 2;
}

enum SessionFailureClass {
  SESSION_FAILURE_CLASS_UNSPECIFIED       = 0;
  CONTAINER_CRASH                          = 1;
  STREAM_DISCONNECT                        = 2;
  GPU_VRAM_EXHAUSTED                       = 3;
  GROUP_QUOTA_EXCEEDED                     = 4;
  NETWORK_POLICY_VIOLATION                 = 5;
  SANDBOX_ESCAPE_ATTEMPT                   = 6;
  FILESYSTEM_BOUNDARY_VIOLATED             = 7;
  STREAMED_SURFACE_IN_CHROME_BLOCKED       = 8;
}

// ─────────────────────────────────────────────────────────────────
// Session container manifest
// ─────────────────────────────────────────────────────────────────

message SessionContainerManifest {
  string session_container_id = 1;
  string group_id              = 2;
  SessionContainerMode mode    = 3;
  SessionContainerRuntime runtime = 4;
  string image_reference       = 5;
  StreamProtocol stream_protocol = 6;
  string sandbox_profile_id    = 7;
  ResourceBudget resource_budget = 8;
  string single_app_action_id  = 9;
  uint32 max_lifetime_seconds  = 10;
  uint32 paused_ttl_seconds    = 11;
  bool is_recovery_container   = 12;
}

message ResourceBudget {
  uint64 ram_bytes_max         = 1;
  uint32 cpu_milli_max         = 2;
  uint64 vram_bytes_max        = 3;
  uint32 stream_bandwidth_kbps_max = 4;
}

// ─────────────────────────────────────────────────────────────────
// Session runtime adapter service
// ─────────────────────────────────────────────────────────────────

message CreateContainerRequest   { SessionContainerManifest manifest = 1; }
message CreateContainerResponse  { string session_container_id = 1; SessionContainerState state = 2; }
message StartContainerRequest    { string session_container_id = 1; }
message StartContainerResponse   { SessionContainerState state = 1; }
message PauseContainerRequest    { string session_container_id = 1; }
message PauseContainerResponse   { SessionContainerState state = 1; }
message ResumeContainerRequest   { string session_container_id = 1; }
message ResumeContainerResponse  { SessionContainerState state = 1; }
message ReclaimContainerRequest  { string session_container_id = 1; bool forensic_snapshot = 2; }
message ReclaimContainerResponse { SessionContainerState state = 1; }
message InspectContainerRequest  { string session_container_id = 1; }
message InspectContainerResponse {
  SessionContainerState state = 1;
  google.protobuf.Timestamp last_transition_at = 2;
  uint64 ram_bytes_observed = 3;
  uint32 cpu_milli_observed = 4;
  uint64 vram_bytes_observed = 5;
  uint32 stream_bandwidth_kbps_observed = 6;
}

service SessionRuntimeAdapter {
  rpc CreateContainer (CreateContainerRequest)  returns (CreateContainerResponse);
  rpc StartContainer  (StartContainerRequest)   returns (StartContainerResponse);
  rpc PauseContainer  (PauseContainerRequest)   returns (PauseContainerResponse);
  rpc ResumeContainer (ResumeContainerRequest)  returns (ResumeContainerResponse);
  rpc ReclaimContainer(ReclaimContainerRequest) returns (ReclaimContainerResponse);
  rpc InspectContainer(InspectContainerRequest) returns (InspectContainerResponse);
}

// ─────────────────────────────────────────────────────────────────
// Signaling protocol (over WebSocket)
// ─────────────────────────────────────────────────────────────────

enum SignalingMessageKind {
  SIGNALING_MESSAGE_KIND_UNSPECIFIED = 0;
  HANDSHAKE_INIT   = 1;
  HANDSHAKE_OK     = 2;
  HANDSHAKE_REJECT = 3;
  PONG             = 4;
  GOODBYE          = 5;
}

message SignalingMessage {
  SignalingMessageKind kind = 1;
  string session_container_id = 2;
  string capability_token = 3;        // populated only for HANDSHAKE_INIT
  StreamProtocol negotiated_protocol = 4;  // populated only for HANDSHAKE_OK
  string reject_reason = 5;            // populated only for HANDSHAKE_REJECT (closed reason code)
}
```
