# Surface + Composition Model (Rev.2)

| Field          | Value                                                                                                                                                                                                                                     |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `CONTRACT` (initial; written 2026-05-10)                                                                                                                                                                                                  |
| Phase tag      | S7.1                                                                                                                                                                                                                                      |
| Layer          | L7 Interaction Renderers                                                                                                                                                                                                                  |
| Schema package | `aios.surface.v1alpha1`                                                                                                                                                                                                                   |
| Consumes       | S0.1 action targets, S1.3 object scope binding, S2.3 policy decisions, S3.1 evidence record schema, S3.2 sandbox profile, S4.1 namespace catalog, S5.1 identity, L0 invariants INV-019..INV-022, L8 GPU resource model (concurrent draft) |
| Produces       | typed `Surface`, closed `SurfaceKind` and `CompositionZone` vocabularies, surface lifecycle FSM, composition rules consumed by every renderer (L7.2 KDE, L7.3 Web, L7.4 CLI, L7.5 Voice, L7.6 Mobile)                                     |

## 1. Purpose

AIOS's interaction surface is **plural by design**: an operator can be in front of a KDE Plasma desktop on a workstation, a localhost web page on a laptop, a CLI on a recovery console, a voice assistant on the phone — all looking at the same authoritative system state. Each renderer shows what it can show; none owns that state.

Beneath all renderers is one architectural primitive: a **Surface**. A Surface is a region (pixels, text grid cells, or audio stream slot) whose contents are owned by one principal — AIOS itself, an app, a stream, or a future XR session — and whose composition with other Surfaces is governed by AIOS, not by the surface's owner. This spec fixes:

1. The closed `SurfaceKind` and `CompositionZone` vocabularies.
2. The Surface lifecycle FSM.
3. The composition rules that every renderer must enforce.
4. The capability brokering hook for GPU access (depends on L8 GPU resource model).
5. Cross-group surface isolation (default-deny, per S4.1).
6. The constitutional placement of AIOS chrome (per INV-020 trust indicators always visible).
7. The recovery-mode surface stack constraint (per INV-022 recovery aesthetic distinct).

This is the foundation under L7.2 Shared UI Schema (consumes Surface as a node kind), L7.3 Web renderer (DOM + WebGPU compositor), L7.2 KDE renderer (KWin + Qt + wgpu), and the deferred renderers.

## 2. Core invariants

- **I1 — Closed Surface vocabulary.** `SurfaceKind` and `CompositionZone` are closed enums. Adding a value requires a versioned spec change.
- **I2 — Surface ownership is constitutional.** Each Surface has exactly one owner (a canonical subject id from L4 identity); the owner cannot be changed in place. Reassigning a Surface is a destroy + recreate.
- **I3 — Composition is renderer-enforced, never surface-controlled.** A Surface owner submits content; the renderer decides z-order, clipping, visibility per the composition rules in §6. App surfaces cannot promote themselves into the CHROME zone.
- **I4 — Cross-group surface isolation.** A Surface owned by a subject in group A cannot be read, listed, or composed alongside a Surface in group B except where S4.1's `_system` audit exception applies. Pixel readback across groups is denied at the GPU layer (L8 GPU model).
- **I5 — AIOS chrome is unbreakable.** The CHROME zone (highest z) is exclusively populated by AIOS itself, never by app surfaces. Renderers reject any APP_SURFACE submission to CHROME with `CompositionZoneForbidden`.
- **I6 — Recovery surface stack is restricted.** When `subject.recovery_mode = true` (per L4 §7), the renderer rejects creation of `APP_SURFACE` and `STREAM_SURFACE` kinds. Only `AIOS_SURFACE` for the recovery shell itself is permitted. This binds INV-022.
- **I7 — Surface lifecycle is FSM-bounded.** `DRAFT → ACTIVE → PAUSED → RETIRED → DESTROYED`. No backward transitions; no skip from `DRAFT` to `RETIRED`. Backward state would imply mutation; AIOS does not mutate identity-bearing records.
- **I8 — Capability-brokered GPU access.** Every Surface that renders through GPU primitives carries a `gpu_capability_class` reference (closed enum from L8 GPU resource model); the renderer obtains a per-group GPU device handle from L8 and binds the Surface to that handle. No raw GPU access bypasses this.
- **I9 — Evidence is emitted per surface lifecycle.** Surface creation, destruction, GPU budget violations, and cross-group access denials are recorded as S3.1 evidence with `STANDARD_24M` retention (or `FOREVER` for tamper-class events).

## 3. The four surface kinds

```proto
enum SurfaceKind {
  SURFACE_KIND_UNSPECIFIED = 0;
  AIOS_SURFACE = 1;       // rendered by AIOS UI schema (L7.2) using DOM/Qt/CLI text
  APP_SURFACE = 2;        // app provides its own renderer; AIOS provides the canvas
  STREAM_SURFACE = 3;     // live media (video, screen sharing, evidence stream)
  XR_SURFACE = 4;         // VR/AR (deferred; reserved enum slot)
}
```

### 3.1 `AIOS_SURFACE`

Rendered by the active renderer's UI schema engine consuming an L7.2 Shared UI Schema tree. The renderer translates the schema to its target primitives:

- KDE renderer: Qt/QML widgets (`QLabel`, `QFormLayout`, `QListView`) for text/list/form node kinds; embedded `wgpu` `QQuickItem` for `Visualization` node kinds.
- Web renderer: DOM (`<form>`, `<input>`, `<ul>`) for text/list/form; `<canvas>` with WebGPU context for `Visualization`.
- CLI renderer: terminal text grid; ANSI styling; box-drawing characters; no GPU.

`AIOS_SURFACE` is the only surface kind allowed in the CHROME zone. AIOS chrome (security indicator, evidence link, AI-vs-human badge per INV-021) is always an AIOS_SURFACE.

### 3.2 `APP_SURFACE`

The owning app provides its own renderer. AIOS provides the surface (a Wayland subsurface on KDE; a `<canvas>` with sandboxed GPU adapter on Web). The app draws into the surface using the GPU capability class permitted by its sandbox profile (S3.2).

Examples:

- A Bevy game compiled to Rust + wgpu (native target) → APP_SURFACE on KDE backed by Wayland subsurface; same source compiled to wasm + WebGPU → APP_SURFACE on Web backed by `<canvas>`.
- A Wine-wrapped Windows game → APP_SURFACE on KDE only (Wine cannot run in browser).
- NeuroCAD using native Vulkan or wgpu → APP_SURFACE on KDE; if the rendering pipeline is portable (wgpu-shaped), also Web.

The app draws; AIOS composites; chrome remains on top.

### 3.3 `STREAM_SURFACE`

A live media stream — a webcam feed, a screen-sharing capture, an evidence playback stream, agent-recorded session replay. Distinguished from APP_SURFACE because the content is consumed (decoded, displayed) rather than produced by an app's renderer; the source of the bytes is the stream protocol.

The renderer decodes per its target capabilities:

- KDE: GStreamer pipeline → dmabuf → Wayland subsurface with hardware-accelerated decoding.
- Web: `<video>` element; for low-latency streams, WebCodecs API → WebGPU texture.

`STREAM_SURFACE` is rejected in recovery mode (no live media in recovery; recovery shows only static system state).

### 3.4 `XR_SURFACE` (deferred)

Reserved for VR/AR rendering targets. Mechanics not specified in Rev.2; the enum slot exists so that future extension does not cause a versioned schema break for consumers that already serialize `SurfaceKind`. Current renderers reject `XR_SURFACE` with `XRDeferred`.

## 4. Composition zones

```proto
enum CompositionZone {
  COMPOSITION_ZONE_UNSPECIFIED = 0;
  BACKGROUND = 1;        // wallpaper, ambient, lowest z
  CONTENT = 2;            // main app and AIOS content surfaces
  OVERLAY = 3;            // notifications, agent messages, ephemeral indicators
  CHROME = 4;             // AIOS chrome — security indicator, action context, recovery banner; highest z
}
```

The zone of a Surface is set at creation and cannot be changed. Migration between zones is destroy + recreate.

### 4.1 Per-zone allowed surface kinds

| Zone       | Allowed kinds                                   | Rationale                                                                 |
| ---------- | ----------------------------------------------- | ------------------------------------------------------------------------- |
| BACKGROUND | `AIOS_SURFACE`                                  | Wallpaper and ambient are AIOS-controlled; apps cannot paint the desktop  |
| CONTENT    | `AIOS_SURFACE`, `APP_SURFACE`, `STREAM_SURFACE` | Main interaction area; both AIOS UI and app surfaces live here            |
| OVERLAY    | `AIOS_SURFACE`                                  | Notifications and agent messages are AIOS-mediated; apps cannot fake them |
| CHROME     | `AIOS_SURFACE` only                             | Constitutional; binds INV-020 (trust indicators always visible)           |

Submissions of disallowed kinds to a zone are rejected with `CompositionZoneForbidden` and emit `CROSS_ZONE_VIOLATION_ATTEMPTED` evidence.

### 4.2 Z-ordering

Strictly: BACKGROUND < CONTENT < OVERLAY < CHROME. Within a zone, ordering is renderer-specific (KDE uses Wayland stack order; Web uses DOM source order with z-index within zone). Within-zone ordering is observable but not constitutional; cross-zone ordering is constitutional.

### 4.3 Mapping to renderer compositor primitives

| Renderer | BACKGROUND                         | CONTENT           | OVERLAY                     | CHROME                                                                 |
| -------- | ---------------------------------- | ----------------- | --------------------------- | ---------------------------------------------------------------------- |
| KDE      | wlr-layer-shell `background` layer | normal wl_surface | wlr-layer-shell `top` layer | wlr-layer-shell `overlay` layer (always on top, even above fullscreen) |
| Web      | DOM `body` background + portal     | DOM article tree  | DOM portal at z-index 9000  | Shadow root at z-index 9999, sandboxed from page DOM                   |
| CLI      | terminal background color          | text body region  | status line                 | top fixed banner with subject + action + evidence ids                  |

KDE's wlr-layer-shell `overlay` layer survives even in fullscreen apps, which is why it is the natural home for CHROME — INV-020 holds even when a game is running fullscreen.

## 5. Surface lifecycle

### 5.1 States

```proto
enum SurfaceLifecycle {
  SURFACE_LIFECYCLE_UNSPECIFIED = 0;
  DRAFT = 1;          // created; not yet visible; capability not yet bound
  ACTIVE = 2;          // visible; rendering in compositor
  PAUSED = 3;          // composited but not receiving updates; e.g., backgrounded app
  RETIRED = 4;          // owner has destroyed; pending GPU resource cleanup
  DESTROYED = 5;        // resources reclaimed; record retained in evidence only
}
```

### 5.2 Allowed transitions

```text
DRAFT     → ACTIVE     (after capability binding succeeds)
DRAFT     → DESTROYED  (owner cancels before activation)
ACTIVE    → PAUSED     (focus loss, minimization, app backgrounded)
PAUSED    → ACTIVE     (focus regained)
ACTIVE    → RETIRED    (owner-initiated destroy)
PAUSED    → RETIRED    (owner-initiated destroy)
RETIRED   → DESTROYED  (resource cleanup completed)
```

Forbidden: `ACTIVE → DRAFT`, `RETIRED → ACTIVE`, `DESTROYED → anything`.

### 5.3 Automatic transitions

- `ACTIVE → PAUSED` on prolonged focus loss > 60 s with no rendering activity (renderer-side optimization).
- `PAUSED → RETIRED` on owning subject's session end (logout, recovery exit, app termination).
- `RETIRED → DESTROYED` on resource reclaim cycle (renderer-defined cadence; default 30 s).

Automatic transitions still emit lifecycle evidence (§9).

## 6. Composition rules

### 6.1 Submission

A Surface owner calls `CreateSurface` (RPC §11) with:

```proto
message CreateSurfaceRequest {
  SurfaceKind kind = 1;
  CompositionZone zone = 2;
  string owner_subject_canonical_id = 3;     // L4 identity
  Dimensions requested_dimensions = 4;
  RenderingHints hints = 5;
  string gpu_capability_class = 6;            // closed enum from L8 GPU resource model
  string namespace_path = 7;                   // S4.1 path under which the surface lives
}
```

The renderer:

1. Validates kind + zone against §4.1 (rejects on mismatch with `CompositionZoneForbidden`).
2. Validates owner against L4 identity (rejects on `OwnerNotResolved`).
3. Resolves `namespace_path` (S4.1) and binds the Surface's `ScopeBinding` (S1.3 §21.1).
4. Checks recovery-mode constraint: if owner is recovery-mode subject, only `AIOS_SURFACE` permitted (§I6). Reject with `RecoveryModeKindForbidden` otherwise.
5. Checks cross-group: if zone allows app surfaces (CONTENT) and the requested zone already hosts surfaces from a different group, the new surface's group_id is recorded but compositor may not pixel-read across groups (§7.4).
6. Requests GPU capability binding from L8 (per `gpu_capability_class`); if denied, surface stays `DRAFT` and propagates the L8 denial reason.
7. Allocates surface*id `surf*<ulid>`and emits`SURFACE_CREATED` evidence.
8. Returns `Surface` record signed by the renderer's identity-service-issued surface signer key.

### 6.2 Composition

Per frame (or per UI update for non-frame-driven renderers like CLI):

```text
1. Iterate zones in order: BACKGROUND, CONTENT, OVERLAY, CHROME.
2. For each zone, gather all ACTIVE surfaces.
3. Apply within-zone ordering (renderer-specific).
4. For each surface:
   - Check ScopeBinding against current rendering session's primary_group_id.
   - If group mismatch and zone is CONTENT: surface contents are NOT rendered; a placeholder
     "surface from group X — switch primary to view" is shown in its place. (Personal flow:
     alice in group A sees surface from group B's persistence layer in her audit trail
     query — the audit trail exists, the pixels do not.)
   - Otherwise: composite the surface's pixels (or text cells) at the within-zone position.
5. AIOS chrome (CHROME zone surfaces) is always rendered last (highest z).
```

### 6.3 Recovery mode composition

When `subject.recovery_mode = true`:

- BACKGROUND: a recovery-mode-only AIOS_SURFACE (per L7.X visual language enforces INV-022 distinct aesthetic).
- CONTENT: only AIOS_SURFACE permitted; recovery shell content. App surfaces and stream surfaces are blocked at creation (§6.1 step 4).
- OVERLAY: only AIOS_SURFACE; recovery-mode notifications.
- CHROME: only AIOS_SURFACE; recovery banner with `_system:operator-<id>` plainly displayed.

The composition pipeline runs as in §6.2 but with the kind restrictions enforced and the recovery aesthetic active.

### 6.4 Fullscreen handling

An app surface may request fullscreen. The renderer enlarges the surface to fill the CONTENT zone but **does not promote it above OVERLAY or CHROME**. Per INV-020, AIOS chrome remains visible even in fullscreen. On KDE, this is enforced by wlr-layer-shell's `overlay` layer; on Web, by the shadow-root chrome that floats above page fullscreen.

A user may temporarily reduce chrome opacity (renderer setting), but the chrome cannot be entirely hidden. Reduction below a renderer-defined floor (e.g., 50%) is rejected.

## 7. Cross-group surface isolation

### 7.1 Pixel-readback isolation

Two surfaces from different groups never share GPU memory. On native (KDE), each group's app surfaces are bound to a distinct `VkDevice` (per L8 GPU resource model); on Web, each group's `APP_SURFACE` uses an isolated `GPUAdapter` (sandboxed by browser origin / iframe). dmabuf passing across group boundaries is denied at L8.

### 7.2 Compositor knowledge

The compositor knows the group of each surface (from `ScopeBinding`). When composing, it can place a group-A surface adjacent to a group-B surface visually but cannot read pixels across the boundary (no screen-recording attack from an app in group A to capture group B).

### 7.3 Cross-group audit reads

A subject in `_system` scope under recovery mode + `system_audit_read` capability + human approver MAY view pixels across groups for audit purposes. This is the same exception as S4.1 §9.2; surface-level enforcement honors the same path.

### 7.4 Denial evidence

A composition that would require cross-group pixel access (e.g., a pixel-shader uniform binding across groups, a screen capture API call from group A targeting group B) emits `CROSS_SURFACE_READ_DENIED` evidence (STANDARD_24M retention) and aborts the operation.

## 8. GPU capability binding (hook to L8)

`gpu_capability_class` on `CreateSurfaceRequest` is a closed enum value defined in L8 GPU Resource Model:

```text
GPU_PASSIVE_DISPLAY      // no shader execution; AIOS just blits framebuffers
GPU_BASIC_2D              // 2D shaders, limited VRAM, low queue priority
GPU_RICH_2D               // full 2D + simple 3D, moderate VRAM, normal queue priority
GPU_FULL_3D               // full GPU access, large VRAM, high queue priority
GPU_COMPUTE_HEAVY         // GPGPU compute access; ML inference, simulation
```

The renderer obtains a `GpuCapabilityBinding` from L8 service for the requesting subject + group + class combination. The binding constrains:

- Per-surface VRAM cap
- Queue priority
- Frame rate cap
- Shader allow/deny lists (where applicable)
- dmabuf authorization peer set

Sandbox profile (S3.2 §18 GPU policy delta, queued for follow-up touch-up) further restricts: an action's SandboxProfile may impose tighter limits than the capability class permits.

A surface exceeding its budget at runtime emits `SURFACE_GPU_BUDGET_EXCEEDED` evidence and is rate-limited (frame skipping) before being demoted to `PAUSED` if violation persists.

## 9. Evidence integration

The following record types are added to S3.1 RecordType vocabulary as part of this contract's adoption:

| Record type                      | Retention class | Carries                                                           |
| -------------------------------- | --------------- | ----------------------------------------------------------------- |
| `SURFACE_CREATED`                | STANDARD_24M    | surface_id, kind, zone, owner_canonical_id, namespace_path, scope |
| `SURFACE_DESTROYED`              | STANDARD_24M    | surface_id, lifecycle reason (owner/auto/error)                   |
| `SURFACE_GPU_BUDGET_EXCEEDED`    | EXTENDED_60M    | surface_id, capability_class, observed vs budgeted resource       |
| `CROSS_SURFACE_READ_DENIED`      | FOREVER         | source_surface_id, target_surface_id, source_group, target_group  |
| `CROSS_ZONE_VIOLATION_ATTEMPTED` | EXTENDED_60M    | surface_id (offender), attempted_zone, attempted_kind             |
| `RECOVERY_KIND_REJECTED`         | FOREVER         | offending owner_canonical_id, attempted_kind                      |

Each record carries `namespace_scope` (per S3.1 §23 touch-up) so cross-group privacy ceiling applies to audit queries.

## 10. Determinism contract

Surface creation is **not** strictly deterministic (allocation involves GPU resource scheduling that depends on system state). However:

```text
GIVEN
  identical CreateSurfaceRequest
  identical subject normalization (L4)
  identical sandbox profile (S3.2)
  identical L8 GPU capability availability
  identical recovery_mode flag

THEN
  the request is either accepted or rejected with the same reason code.
```

This is the **decision determinism** — the same request, in the same system state, gets the same accept/reject decision and the same reason code. The actual `surface_id` is a fresh ULID; that is not part of the determinism guarantee.

## 11. Performance contract

| Operation                                      | p50      | p95      | p99      | Hard timeout  |
| ---------------------------------------------- | -------- | -------- | -------- | ------------- |
| `CreateSurface` (AIOS_SURFACE, no GPU)         | < 1 ms   | < 5 ms   | < 20 ms  | 200 ms        |
| `CreateSurface` (APP_SURFACE, with GPU)        | < 10 ms  | < 50 ms  | < 200 ms | 2 s           |
| Per-frame compositor pass (KDE; ≤ 16 surfaces) | < 1 ms   | < 4 ms   | < 8 ms   | 16 ms (60 Hz) |
| Per-frame compositor pass (Web; ≤ 16 surfaces) | < 1 ms   | < 5 ms   | < 16 ms  | 16 ms (60 Hz) |
| `DestroySurface`                               | < 5 ms   | < 50 ms  | < 200 ms | 1 s           |
| Cross-group denial fast-path                   | < 100 µs | < 500 µs | < 2 ms   | 10 ms         |

Failure modes — all fail closed:

- `RendererInternal` → caller receives error; engine emits alert.
- `GpuCapabilityUnavailable` → request stays `DRAFT`; caller can retry with lower capability class.
- `CompositionBackpressure` → renderer rejects new surfaces when active count exceeds threshold (default 256 active surfaces total).

## 12. Adversarial robustness

### 12.1 Surface-id forgery

Surface records are signed by the renderer at creation. Forged surface ids that don't match the renderer's signing key are rejected at every consumer (compositor, evidence log, query language).

### 12.2 Zone promotion attacks

An app submitting CHROME zone is rejected with `CompositionZoneForbidden`. The renderer never trusts the requested zone; it validates against §4.1 and refuses promotions.

### 12.3 GPU exfiltration

Cross-group pixel readback is blocked at L8 (per-group VkDevice / per-origin GPUAdapter). dmabuf passing checks the recipient's group; mismatches fail. Compute shaders attempting to read framebuffer textures across group boundaries fail device-level access checks.

### 12.4 Chrome impersonation

Apps may design their surfaces to look like AIOS chrome. The renderer mitigates this by enforcing a constant CHROME zone overlay that covers the canonical chrome rectangles regardless of app design. The CHROME zone surfaces are rendered last; an app surface trying to draw a fake security indicator is then over-painted by the real one.

### 12.5 Recovery mode escape

If a subject's recovery_mode flag flips during a session (which should never happen — L4 §7.2 makes this impossible), running APP_SURFACE/STREAM_SURFACE instances are immediately retired; the compositor refuses to render them. The recovery-mode kind constraint is checked at every frame, not only at creation.

### 12.6 Surface-flood DoS

A subject may not own more than `max_active_surfaces_per_subject` (default 32) surfaces simultaneously. Excess creation requests are rejected with `SurfaceQuotaExceeded`. The quota is per-subject, not per-group, to prevent one subject from starving its peers.

### 12.7 Frame skipping detection

The compositor records per-frame surface inclusion decisions in a rotating in-memory buffer. A surface that is never composited despite being ACTIVE for > 5 s emits `SURFACE_NEVER_RENDERED` evidence (added to S3.1 vocabulary as part of this contract — STANDARD_24M retention) — this catches renderer bugs and intentional drop attacks.

## 13. Cross-spec dependencies

| Spec                         | Direction | What this spec contributes                                                                                                                                                                                                        |
| ---------------------------- | --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S0.1                         | producer  | Action targets may reference `surf_<ulid>` paths (under namespace `groups/<g>/agents/<a>/runtime/surfaces/`)                                                                                                                      |
| S1.3                         | producer  | Surface objects carry `ScopeBinding` per S1.3 §21.1; cross-scope pointer moves remain forbidden                                                                                                                                   |
| S2.1                         | producer  | New closed query field `target.surface_kind` and `target.composition_zone`; surface lifecycle queryable                                                                                                                           |
| S2.3                         | producer  | New closed condition fields `target.surface_kind`, `target.composition_zone`; constitutional hard-deny `CompositionZoneForbidden` candidate (NOT applied here; flagged for follow-up)                                             |
| S2.4                         | producer  | New primitive `surface_in_zone(surface_id, expected_zone)` for property checks (queued for follow-up touch-up)                                                                                                                    |
| S3.1                         | producer  | Seven new record types (`SURFACE_CREATED`, `SURFACE_DESTROYED`, `SURFACE_GPU_BUDGET_EXCEEDED`, `CROSS_SURFACE_READ_DENIED` FOREVER, `CROSS_ZONE_VIOLATION_ATTEMPTED`, `RECOVERY_KIND_REJECTED` FOREVER, `SURFACE_NEVER_RENDERED`) |
| S3.2                         | producer  | New field `gpu_policy` on `SandboxProfile` (queued; full delta in follow-up); apply-time check that surface's group matches subject's primary_group_id                                                                            |
| S4.1                         | consumer  | Surface namespace_path validated by namespace resolver; `groups/<g>/agents/<a>/runtime/surfaces/` is the canonical location                                                                                                       |
| S5.1                         | consumer  | Owner is a `canonical_subject_id` from L4 identity; `recovery_mode` flag from session drives §6.3                                                                                                                                 |
| L0 INV-020                   | enforcer  | This spec implements the chrome-cannot-be-hidden invariant in §I5 + §6.4                                                                                                                                                          |
| L0 INV-022                   | enforcer  | This spec implements the recovery-mode-distinct invariant in §I6 + §6.3                                                                                                                                                           |
| L8 GPU resource model        | consumer  | `gpu_capability_class` enum and `GpuCapabilityBinding` are defined in L8; this spec consumes them                                                                                                                                 |
| L7.2 KDE renderer (deferred) | consumer  | Translates Surface model to KWin + Wayland subsurfaces + wlr-layer-shell                                                                                                                                                          |
| L7.3 Web renderer (deferred) | consumer  | Translates Surface model to DOM portals + WebGPU canvas + shadow-root chrome                                                                                                                                                      |

## 14. Golden fixtures

### Fixture 1 — AIOS evidence viewer surface (AIOS_SURFACE in CONTENT)

```text
CreateSurfaceRequest {
  kind: AIOS_SURFACE,
  zone: CONTENT,
  owner_subject_canonical_id: "family:alice",
  gpu_capability_class: GPU_BASIC_2D,
  namespace_path: "/aios/groups/family/users/alice/desktop/evidence-viewer/surf-001"
}

Expected:
  result: Surface { surface_id: "surf_<ulid>", lifecycle: ACTIVE }
  composition: composited in CONTENT zone, below OVERLAY and CHROME
  evidence emitted: SURFACE_CREATED with namespace_scope = (GROUP, family, alice)
```

### Fixture 2 — Bevy game APP_SURFACE on KDE

```text
CreateSurfaceRequest {
  kind: APP_SURFACE,
  zone: CONTENT,
  owner_subject_canonical_id: "family:app:com.example.game:i-01",
  gpu_capability_class: GPU_FULL_3D,
  namespace_path: "/aios/groups/family/apps/com.example.game/runtime/surfaces/main"
}

Renderer: KDE
Expected:
  L8 issues GpuCapabilityBinding for VkDevice on family group's GPU partition.
  Wayland subsurface created; dmabuf channel established with the game process.
  Surface composited in CONTENT zone; KWin layer-shell ensures CHROME overlay remains on top
  even when game enters fullscreen.
  Game's wgpu code renders directly into the dmabuf-backed texture.
  AIOS security indicator (CHROME zone, AIOS_SURFACE) shows "family:app:com.example.game running, action: render-frame, evidence: evr_..."
```

### Fixture 3 — NeuroCAD APP_SURFACE on KDE and Web

```text
NeuroCAD ships a Rust + wgpu rendering core.

CreateSurfaceRequest {
  kind: APP_SURFACE,
  zone: CONTENT,
  owner_subject_canonical_id: "homelab:app:bg.iconys.neurocad:i-01",
  gpu_capability_class: GPU_RICH_2D + co-binding GPU_COMPUTE_HEAVY,
  namespace_path: "/aios/groups/homelab/apps/bg.iconys.neurocad/runtime/surfaces/canvas"
}

KDE renderer:
  Native binary using Vulkan via wgpu; wlr_subcompositor surface; dmabuf-backed.

Web renderer:
  Wasm binary using WebGPU via wgpu (wasm32 target); browser <canvas> with WebGPU context;
  GPUAdapter sandboxed by origin.

Same source compiled twice; AIOS surface contract is identical on both targets.
```

### Fixture 4 — Cross-group surface read denied

```text
Setup:
  alice in family group has surface_a in CONTENT zone.
  bob's app in homelab group attempts pixel readback of surface_a via screen-capture API.

Expected:
  L8 GPU device check: surface_a is on family-group's VkDevice; bob's app is on homelab-group's VkDevice.
  Read attempt fails at device-isolation layer.
  CROSS_SURFACE_READ_DENIED evidence emitted (FOREVER retention) with
  source = "homelab:app:...", target = surface_a, source_group = "homelab", target_group = "family".
```

### Fixture 5 — APP_SURFACE rejected in recovery mode

```text
Setup:
  Subject _system:local:operator-247 in recovery_mode = true.
  Recovery shell active.

  An app installed in family group attempts to create an APP_SURFACE through some
  pre-existing capability (which it should not have, but defense in depth).

CreateSurfaceRequest { kind: APP_SURFACE, ... }
Expected:
  Renderer rejects with RecoveryModeKindForbidden.
  RECOVERY_KIND_REJECTED evidence emitted (FOREVER retention).
  Operator's recovery shell is undisturbed; only recovery AIOS_SURFACE remains.
```

### Fixture 6 — Chrome remains above fullscreen game

```text
APP_SURFACE in CONTENT zone enters fullscreen.
KDE: surface enlarged to fill CONTENT zone; wlr-layer-shell `overlay` layer (CHROME) remains visible above.
Web: page enters fullscreen; shadow-root CHROME at z-index 9999 remains visible above page fullscreen.

INV-020 satisfied: trust indicator never disappears.
```

### Fixture 7 — Surface-id forgery rejected

```text
Caller submits an action with target.surface_id = "surf_<forged-ulid>" not signed by the renderer.
Compositor checks signature against renderer signing key; mismatch.
Action rejected at S0.1 acceptance with InvalidTargetPath (surface_id resolves to a non-existent or unsigned record).
```

### Fixture 8 — Surface-flood DoS prevented

```text
A misbehaving subject attempts to create 1000 surfaces.
After 32 active surfaces, further CreateSurface returns SurfaceQuotaExceeded.
Subject is not blacklisted (the surfaces are owned by them; quota is the natural rate limit).
Once existing surfaces are destroyed, new creation is allowed again.
```

## 15. Telemetry contract

All metrics MUST use bounded label cardinality. **surface_id, owner_subject_canonical_id, group_id, namespace_path are NEVER labels.**

| Metric                                  | Type      | Labels (closed)                                          |
| --------------------------------------- | --------- | -------------------------------------------------------- |
| `surface_create_total`                  | counter   | `kind`, `zone`, `result` (success/error), `error_code`   |
| `surface_active`                        | gauge     | `kind`, `zone`                                           |
| `surface_create_duration_seconds`       | histogram | `kind`                                                   |
| `surface_destroy_total`                 | counter   | `reason_class` (owner/auto/error)                        |
| `surface_lifecycle_transition_total`    | counter   | `from_state`, `to_state`                                 |
| `surface_gpu_budget_exceeded_total`     | counter   | `capability_class`                                       |
| `surface_cross_group_read_denied_total` | counter   | none                                                     |
| `surface_zone_violation_total`          | counter   | `attempted_zone`, `attempted_kind`                       |
| `surface_recovery_kind_rejected_total`  | counter   | `attempted_kind`                                         |
| `surface_never_rendered_total`          | counter   | `kind`, `zone`                                           |
| `surface_quota_exceeded_total`          | counter   | none                                                     |
| `compositor_frame_duration_seconds`     | histogram | `renderer` (kde/web/cli), `zone_count_class` (1/4/16/64) |

Cardinality budget: ≤ 200 active label tuples per metric.

## 16. Acceptance criteria

- [ ] `SurfaceKind` is a closed enum with four values (XR_SURFACE deferred but reserved).
- [ ] `CompositionZone` is a closed enum with four values; per-zone allowed kinds match §4.1.
- [ ] Surface lifecycle FSM has five states with the exact transitions in §5.2; forbidden transitions rejected.
- [ ] AIOS chrome (CHROME zone) is enforced as always-on-top; rejected app submissions to CHROME emit `CROSS_ZONE_VIOLATION_ATTEMPTED` evidence.
- [ ] Recovery-mode subject creation of APP_SURFACE / STREAM_SURFACE rejected with `RecoveryModeKindForbidden` and FOREVER-retained evidence.
- [ ] Cross-group surface pixel readback denied at GPU layer (depends on L8 implementation); `CROSS_SURFACE_READ_DENIED` evidence FOREVER-retained.
- [ ] Surface signatures verified by every consumer (compositor, evidence log, S0.1 envelope acceptance).
- [ ] Per-subject `max_active_surfaces_per_subject` quota (default 32) enforced.
- [ ] All eight golden fixtures (§14) produce the specified outcomes.
- [ ] Telemetry conforms to §15; surface_id / owner / group / namespace_path never appear as labels.
- [ ] L0 INV-019 (visual identity preserved) addressed via composition rules being renderer-agnostic in this spec.
- [ ] L0 INV-020 (trust indicators always visible) implemented via CHROME-zone constraints.
- [ ] L0 INV-022 (recovery aesthetic distinct) implemented via §6.3 recovery surface stack restriction.

## 17. Open deferrals

- **`XR_SURFACE` mechanics** — VR/AR rendering targets. Reserved enum slot only; full mechanics deferred to a future spec.
- **Multi-monitor topology** — surface-to-monitor binding, migration between monitors. Deferred.
- **HDR / wide-gamut color profiles** — per-surface color management. Deferred to L7.X visual language and per-renderer details.
- **Surface migration between renderers** — moving an active session from KDE to Web mid-flight. Deferred; involves session state serialization and identity continuity.
- **Surface recording / screen capture** — security-sensitive; requires careful capability design (record-with-consent, evidence trail of every recorded session). Deferred.
- **Variable refresh rate / VRR** — per-surface frame pacing hints. Deferred to L7 renderer specs.
- **Per-surface accessibility tree (a11y)** — for AIOS_SURFACE this is straightforward (DOM/Qt accessibility); for APP_SURFACE it requires app cooperation and an accessibility delegation protocol. Deferred.
- **Surface privilege levels for trusted sources** — e.g., system services can render special chrome elements. Currently CHROME is AIOS-only with no further sub-distinction. Deferred.
- **Composition policy bundles** — operator-configurable composition rules (e.g., "OVERLAY notifications timeout after 5 s in finance group"). Deferred.

## Appendix A — Full Proto IDL

```proto
syntax = "proto3";
package aios.surface.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/duration.proto";

// ============================================================================
// Service
// ============================================================================

service SurfaceService {
  rpc CreateSurface(CreateSurfaceRequest) returns (CreateSurfaceResponse);
  rpc DestroySurface(DestroySurfaceRequest) returns (DestroySurfaceResponse);
  rpc PauseSurface(PauseSurfaceRequest) returns (PauseSurfaceResponse);
  rpc ResumeSurface(ResumeSurfaceRequest) returns (ResumeSurfaceResponse);
  rpc GetSurface(GetSurfaceRequest) returns (GetSurfaceResponse);
  rpc ListSurfaces(ListSurfacesRequest) returns (ListSurfacesResponse);
  rpc GetSurfaceServiceInfo(GetSurfaceServiceInfoRequest) returns (GetSurfaceServiceInfoResponse);
}

// ============================================================================
// Core types
// ============================================================================

enum SurfaceKind {
  SURFACE_KIND_UNSPECIFIED = 0;
  AIOS_SURFACE = 1;
  APP_SURFACE = 2;
  STREAM_SURFACE = 3;
  XR_SURFACE = 4;
}

enum CompositionZone {
  COMPOSITION_ZONE_UNSPECIFIED = 0;
  BACKGROUND = 1;
  CONTENT = 2;
  OVERLAY = 3;
  CHROME = 4;
}

enum SurfaceLifecycle {
  SURFACE_LIFECYCLE_UNSPECIFIED = 0;
  DRAFT = 1;
  ACTIVE = 2;
  PAUSED = 3;
  RETIRED = 4;
  DESTROYED = 5;
}

message Dimensions {
  uint32 width_logical_pixels = 1;
  uint32 height_logical_pixels = 2;
  float scale_factor = 3;             // for HiDPI; 1.0 = standard
}

message RenderingHints {
  bool prefers_low_latency = 1;
  bool prefers_high_quality = 2;
  uint32 target_fps = 3;               // 0 = renderer default (typically 60)
  bool transparent_background = 4;
  bool clip_to_zone = 5;               // default true
}

message ScopeBinding {                  // mirrors S1.3 §21.1
  string scope_kind = 1;
  string group_id = 2;
  string user_id = 3;
}

message Surface {
  string surface_id = 1;                // surf_<ulid>
  SurfaceKind kind = 2;
  CompositionZone zone = 3;
  string owner_subject_canonical_id = 4;
  string namespace_path = 5;
  ScopeBinding scope_binding = 6;
  Dimensions dimensions = 7;
  RenderingHints hints = 8;
  string gpu_capability_class = 9;      // closed enum from L8
  string gpu_capability_binding_id = 10; // L8 binding reference
  SurfaceLifecycle lifecycle = 11;
  google.protobuf.Timestamp created_at = 12;
  google.protobuf.Timestamp last_active_at = 13;
  bytes ed25519_signature = 14;         // signed by renderer
}

// ============================================================================
// RPC request/response
// ============================================================================

message CreateSurfaceRequest {
  SurfaceKind kind = 1;
  CompositionZone zone = 2;
  string owner_subject_canonical_id = 3;
  Dimensions requested_dimensions = 4;
  RenderingHints hints = 5;
  string gpu_capability_class = 6;
  string namespace_path = 7;
}

message CreateSurfaceResponse {
  oneof result {
    Surface surface = 1;
    CreateSurfaceError error = 2;
  }
}

enum CreateSurfaceErrorCode {
  CREATE_SURFACE_ERROR_CODE_UNSPECIFIED = 0;
  COMPOSITION_ZONE_FORBIDDEN = 1;
  OWNER_NOT_RESOLVED = 2;
  RECOVERY_MODE_KIND_FORBIDDEN = 3;
  GPU_CAPABILITY_UNAVAILABLE = 4;
  XR_DEFERRED = 5;
  SURFACE_QUOTA_EXCEEDED = 6;
  COMPOSITION_BACKPRESSURE = 7;
  INVALID_NAMESPACE_PATH = 8;
  SCOPE_BINDING_MISMATCH = 9;
  RENDERER_INTERNAL = 10;
}

message CreateSurfaceError {
  CreateSurfaceErrorCode code = 1;
  string message = 2;
  string l8_denial_reason = 3;          // populated when code = GPU_CAPABILITY_UNAVAILABLE
}

message DestroySurfaceRequest { string surface_id = 1; string reason = 2; }
message DestroySurfaceResponse { bool destroyed = 1; }

message PauseSurfaceRequest { string surface_id = 1; }
message PauseSurfaceResponse { bool paused = 1; }

message ResumeSurfaceRequest { string surface_id = 1; }
message ResumeSurfaceResponse { bool resumed = 1; }

message GetSurfaceRequest { string surface_id = 1; }
message GetSurfaceResponse {
  oneof result {
    Surface surface = 1;
    string error_message = 2;
  }
}

message ListSurfacesRequest {
  string owner_subject_canonical_id = 1;  // optional filter
  CompositionZone zone = 2;                // optional filter
  SurfaceKind kind = 3;                    // optional filter
  uint32 page_size = 4;
  string cursor = 5;
}
message ListSurfacesResponse {
  repeated Surface surfaces = 1;
  string next_cursor = 2;
  uint32 suppressed_count = 3;             // cross-group privacy ceiling
}

message GetSurfaceServiceInfoRequest {}
message GetSurfaceServiceInfoResponse {
  string renderer_id = 1;                  // "kde" | "web" | "cli" | etc.
  string schema_version = 2;               // "aios.surface.v1alpha1"
  uint64 active_surface_count = 3;
  uint64 max_active_surfaces_total = 4;
  uint32 max_active_surfaces_per_subject = 5;
}
```

## See also

- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S1.3 — AIOS-FS Object Model](../L2_AIOS_FS/01_object_model.md)
- [S2.1 — Query/View Language](../L2_AIOS_FS/02_query_view_language.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S3.2 — Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [S4.1 — Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S5.1 — Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [S6.4 — Constitutional Invariants (incl. INV-019..INV-022)](../L0_Governance_Evidence_Safety/04_invariants.md)
- [L7 Overview](00_overview.md)
- [L8 GPU Resource Model](../L8_Network_Hardware_Devices/05_gpu_resource_model.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
