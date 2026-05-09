# GPU Resource Model (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                  |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `CONTRACT` (initial; written 2026-05-10)                                                                                                                                                                                                                                                                                                                               |
| Phase tag      | S8.2                                                                                                                                                                                                                                                                                                                                                                   |
| Layer          | L8 Network, Hardware, Devices                                                                                                                                                                                                                                                                                                                                          |
| Schema package | `aios.gpu.v1alpha1`                                                                                                                                                                                                                                                                                                                                                    |
| Consumes       | S0.1 (action targets that need GPU), S1.3 (object scope binding), S2.3 (policy decisions reference `GpuCapabilityClass`), S3.2 (sandbox profile `GpuPolicy` field — cross-spec touch-up queued), S4.1 (group scoping for `VkDevice` partitioning), S5.1 (subject identity for capability binding), S7.1 (Surface `gpu_capability_class` consumer)                      |
| Produces       | typed `GpuDevice` topology, closed `GpuCapabilityClass` enum (5 values), `GpuCapabilityBinding`, dmabuf brokering rules, VRAM accounting per `(group, subject, capability_class)`, per-group `VkDevice` partitioning protocol on native, per-origin `GPUAdapter` sandboxing on Web, and the failure mode catalogue when GPU resources are exhausted, lying, or revoked |

## 1. Purpose

AIOS treats the GPU as a **managed, brokered resource**, not as a raw device that any process may open. Every shader compiled, every framebuffer allocated, every dmabuf passed across a process boundary flows through a typed capability that names a group, a subject, and one of five precisely-bounded capability classes. Without this contract:

- S7.1 Surface + Composition Model has a dangling `gpu_capability_class` reference.
- S3.2 Sandbox Composition cannot enforce per-action GPU limits — every sandbox is silently allowed full GPU.
- Cross-group surface pixel-readback isolation (S7.1 §I4) has no implementation point: pixel isolation lives at the GPU device boundary, not in the compositor.
- Multi-user / multi-group concurrent rendering on a single physical GPU has no resource-accounting model.

This spec fixes:

1. The closed `GpuCapabilityClass` vocabulary (5 values) and per-class budget table.
2. The `GpuDevice` topology (vendor / model / VRAM / supported APIs) and the enumeration protocol.
3. Per-group `VkDevice` partitioning on native renderers (KDE, future XR) using `wgpu` over Vulkan / Metal / DX12.
4. Per-origin `GPUAdapter` sandboxing on the Web renderer using browser origin / iframe isolation.
5. dmabuf brokering: who may receive a dmabuf handle from whom, and the default-deny across group boundaries.
6. VRAM accounting per `(group, subject, capability_class)` and the demotion behaviour under pressure.
7. Capability binding lifecycle, signature, and revocation.
8. Hardware-capability-lie detection (driver claims one ABI, kernel exposes another).
9. The failure mode catalogue and the performance budgets.

This is the L8 sibling of S7.1: surfaces consume what this contract produces. Both were identified by the architectural brainstorm of 2026-05-10 as required foundation contracts before downstream renderers (L7.4 KDE, L7.5 Web) and sandbox GPU policy (S3.2 cross-spec touch-up) can be written rigorously.

## 2. Core invariants

- **I1 — Closed capability vocabulary.** `GpuCapabilityClass` is a closed enum with exactly five values (§3). Adding or removing a class is a versioned schema change. There is no "custom" class.
- **I2 — No raw GPU access.** Every shader-bearing process — AIOS itself, an app instance, an agent runtime, a renderer — obtains a `GpuCapabilityBinding` from the L8 GPU service before issuing a single GPU command. Direct opening of `/dev/dri/*` by anything other than the L8 GPU service is denied at the sandbox layer (S3.2).
- **I3 — Per-group device isolation on native.** On a native renderer (KDE / XR), each active group receives its own `VkDevice` (or platform equivalent: `MTLDevice` on macOS, `ID3D12Device` on Windows-via-Wine). Two groups never share a `VkDevice`. Memory allocated under group A's `VkDevice` is not addressable from group B's `VkDevice`. This binds S4.1's cross-group default-deny to the GPU memory boundary.
- **I4 — Per-origin sandboxing on Web.** On the Web renderer, each active group's surfaces run in an iframe whose origin is `https://aios.localhost/<group_id>/`. Each origin therefore receives its own browser-managed `GPUAdapter`. The browser's same-origin policy does the cross-group pixel isolation; AIOS verifies the iframe origin matches the surface's `ScopeBinding` at composition time.
- **I5 — dmabuf passing is brokered.** A dmabuf file descriptor produced under one `VkDevice` cannot be imported by another `VkDevice` without an explicit `AuthorizeDmabufPeer` grant from the L8 service. Cross-group dmabuf grants are denied by default (§6); same-group grants are allowed but recorded.
- **I6 — VRAM accounting is per-tuple, not per-surface.** The accounting key is `(group_id, subject_canonical_id, capability_class)`. A subject that creates 100 surfaces does not get 100× the VRAM budget; the surfaces share the subject's per-class allocation.
- **I7 — Capability binding is signed and verifiable.** Every `GpuCapabilityBinding` carries an Ed25519 signature from the L8 service signing key. Consumers (S7.1 surface service, S3.2 sandbox enforcement, the renderer's GPU command submission path) verify the signature before trusting the binding.
- **I8 — Hardware capability lies are detected and refused.** If the driver reports support for an API version that the kernel does not actually expose (e.g., driver claims Vulkan 1.3, kernel `/proc/.../vulkan` reports 1.1), the L8 service refuses bindings against the lying class and emits `HOST_CAPABILITY_LIE` evidence with `FOREVER` retention.
- **I9 — Failure is closed.** Every failure mode (`GpuExhausted`, `DriverUnavailable`, `IommuUnavailable`, `CapabilityClassDenied`, `DmabufCrossGroupForbidden`, `BindingSignatureInvalid`, `HostCapabilityLie`) results in binding refusal. There is no "best effort" or "fall back to silently lower class without telling the caller". A demotion (e.g., `GPU_FULL_3D` → `GPU_RICH_2D`) is an explicit success of a lower class, recorded as `GPU_BUDGET_DOWNGRADED` evidence; it is never silent.
- **I10 — Recovery mode disables GPU validation layers.** In recovery boot, Vulkan validation layers (and the equivalent on other backends) are disabled to reduce attack surface; this is recorded as `GPU_VALIDATION_DISABLED_RECOVERY` evidence. Outside recovery, validation is on by default.
- **I11 — Compositor is a privileged dmabuf peer.** The KDE compositor (KWin) running under `_system:kwin` may receive dmabuf handles from any group's surface for the purpose of composition. The compositor performs **opaque** composition only: it never copies pixels into another group's `VkDevice`. This is the GPU-layer expression of S7.1 §7.2.

## 3. The five GPU capability classes

```proto
enum GpuCapabilityClass {
  GPU_CAPABILITY_CLASS_UNSPECIFIED = 0;
  GPU_PASSIVE_DISPLAY = 1;
  GPU_BASIC_2D = 2;
  GPU_RICH_2D = 3;
  GPU_FULL_3D = 4;
  GPU_COMPUTE_HEAVY = 5;
}
```

Each class is a tight bundle of permissions; downgrade is allowed (a `GPU_FULL_3D` request may be served by a `GPU_RICH_2D` binding when budget is tight, with explicit evidence), upgrade is never automatic.

### 3.1 Class budgets

| Class                 | Shader operations                                                                      | VRAM cap (default) | VRAM minimum | Queue priority | Frame rate cap | Compute pipeline |
| --------------------- | -------------------------------------------------------------------------------------- | ------------------ | ------------ | -------------- | -------------- | ---------------- |
| `GPU_PASSIVE_DISPLAY` | none — blit-only                                                                       | 16 MiB             | 4 MiB        | `NICE`         | 30 FPS         | denied           |
| `GPU_BASIC_2D`        | 2D shaders only (vertex + fragment, no geometry, no compute, no tessellation)          | 64 MiB             | 16 MiB       | `NICE`         | 60 FPS         | denied           |
| `GPU_RICH_2D`         | 2D + simple 3D (no tessellation, no mesh shaders, no ray tracing, no compute)          | 256 MiB            | 64 MiB       | `NORMAL`       | 60 FPS         | denied           |
| `GPU_FULL_3D`         | full graphics pipeline incl. geometry, tessellation, mesh shaders, ray tracing         | 25% of total VRAM  | 256 MiB      | `HIGH`         | 144 FPS        | denied           |
| `GPU_COMPUTE_HEAVY`   | compute pipelines (CUDA via wgpu-cuda, HIP, Metal Performance Shaders, WebGPU compute) | 50% of total VRAM  | 512 MiB      | `HIGH`         | n/a (headless) | allowed          |

`NICE` / `NORMAL` / `HIGH` is a closed enum (§Appendix A `QueuePriority`). On Vulkan, `NICE` maps to `VK_QUEUE_GLOBAL_PRIORITY_LOW_KHR`, `NORMAL` to `VK_QUEUE_GLOBAL_PRIORITY_MEDIUM_KHR`, `HIGH` to `VK_QUEUE_GLOBAL_PRIORITY_HIGH_KHR`. On WebGPU there is no queue priority knob; the class is recorded for accounting only.

VRAM caps marked as percentages are computed against the total VRAM of the GPU the binding is issued against, with a hard minimum from the column. `25% of total VRAM` on a 16 GiB GPU is 4 GiB; on a 4 GiB integrated GPU the minimum 256 MiB applies (no further reduction; if even 256 MiB is unavailable, the class is denied).

The defaults are policy-overridable per group in the active policy bundle (S2.3 §6.X — touch-up queued); this spec defines the **floor** values.

### 3.2 dmabuf authorization peer set per class

| Source class          | Default authorized peers                                                              | Notes                                                                            |
| --------------------- | ------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `GPU_PASSIVE_DISPLAY` | `_system:kwin` only                                                                   | A passive display surface produces a single composition handoff; nothing else    |
| `GPU_BASIC_2D`        | `_system:kwin`, surfaces of the same `(group, subject)`                               | Same-subject sharing supported (e.g., one app's main + popover surface)          |
| `GPU_RICH_2D`         | `_system:kwin`, surfaces of the same `(group, subject)`, surfaces of the same `group` | Cross-subject within group allowed (e.g., screen-share within a homelab session) |
| `GPU_FULL_3D`         | `_system:kwin`, surfaces of the same `(group, subject)`                               | High-bandwidth content; no cross-subject sharing without explicit policy         |
| `GPU_COMPUTE_HEAVY`   | `_system:kwin` (only if a render output exists), same `(group, subject)` only         | Compute output buffers stay within the originating subject by default            |

Cross-group peers always require explicit `AuthorizeDmabufPeer` with policy approval; the default is denial with `DmabufCrossGroupForbidden` (§6).

### 3.3 Why exactly five classes

Three would not be enough: 2D vs 3D vs compute conflates "weather widget" with "AAA game" and conflates "NeuroCAD viewport" with "ML training run" — both pairs need different VRAM and queue characteristics. Seven would over-fit: distinguishing "2D with shaders" from "2D with shaders and one filter" yields no operator-meaningful difference. Five matches the empirical breakdown of GPU work on a personal/homelab system: pure presentation, simple chrome, productive UI, real 3D, and compute.

## 4. Device topology

### 4.1 `GpuDevice` message

```proto
message GpuDevice {
  string device_id = 1;                         // gpu_<ulid>
  GpuKind kind = 2;                              // INTEGRATED | DISCRETE
  string vendor = 3;                             // "NVIDIA" | "AMD" | "Intel" | ...
  string model = 4;                              // "RTX 4090" | "RX 7900 XTX" | ...
  string driver_name = 5;                        // "nvidia" | "radv" | "i915"
  string driver_version = 6;                     // "555.42.02"
  uint64 vram_total_bytes = 7;
  uint64 vram_currently_allocated_bytes = 8;
  GpuApiSupport api_support = 9;
  bool iommu_enforced = 10;                      // true = cross-VkDevice memory isolation hard; false = degraded
  google.protobuf.Timestamp first_seen_at = 11;
  google.protobuf.Timestamp last_health_check_at = 12;
}

enum GpuKind {
  GPU_KIND_UNSPECIFIED = 0;
  INTEGRATED = 1;
  DISCRETE = 2;
}

message GpuApiSupport {
  string vulkan_version = 1;                     // "1.3" or "" if unsupported
  string opengl_version = 2;                     // "4.6" or ""
  string opencl_version = 3;                     // "3.0" or ""
  string metal_version = 4;                      // "" on Linux
  string dx12_version = 5;                       // "" on Linux
  bool webgpu_supported = 6;                     // via wgpu wasm in Web renderer
  bool ray_tracing_supported = 7;
  bool mesh_shader_supported = 8;
}
```

### 4.2 Enumeration

`EnumerateDevices` is called by the L8 GPU service at boot, on hotplug events from udev, and on operator request. Results are cached for the session lifetime; the cache is invalidated on hotplug. Each enumeration emits one `GPU_DEVICE_ENUMERATED` evidence record (`STANDARD_24M`) per device discovered, and `GPU_DEVICE_DISCONNECTED` (`STANDARD_24M`) per device removed.

### 4.3 Multi-GPU systems

Personal workstations and small homelab boxes routinely have two GPUs: an integrated GPU on the CPU package (Intel iGPU / AMD APU) and a discrete GPU on PCIe. AIOS handles this as follows:

- **Routing by class.** `GPU_PASSIVE_DISPLAY` and `GPU_BASIC_2D` requests go to the integrated GPU when one is present; this keeps the discrete GPU idle for `GPU_FULL_3D` and `GPU_COMPUTE_HEAVY` work. `GPU_RICH_2D` goes to whichever has free VRAM; the integrated GPU is preferred to keep the discrete GPU available for higher classes. `GPU_FULL_3D` and `GPU_COMPUTE_HEAVY` go to the discrete GPU.
- **Per-GPU partitioning.** Each physical GPU runs the per-group `VkDevice` partitioning of §5 independently. Group A may have a `VkDevice` on the integrated GPU and another `VkDevice` on the discrete GPU concurrently; both are accounted under group A's budget but on different devices.
- **Policy override.** A policy may pin a group's class to a specific `device_id`; the binding fails with `CapabilityClassDenied` if that device is unavailable, rather than silently falling back.

## 5. Per-group `VkDevice` partitioning protocol (native)

### 5.1 When a `VkDevice` is created

On the first `IssueCapabilityBinding` request from group A against `device_id = X`, the L8 service:

1. Verifies group A is recognised by L4 identity (S5.1) and is currently active (at least one authenticated session exists in group A).
2. Reserves the per-group VRAM budget on device X (default: 25% of the device's total VRAM; minimum 256 MiB; configurable per group via the active policy bundle's `group_floor` source per S3.2 §18).
3. Calls `vkCreateDevice` with the queue family selection that matches the requested `QueuePriority` and with the validation layer set per recovery flag (§I10).
4. Caches the resulting `VkDevice` keyed by `(group_id, device_id)`.
5. Emits `GPU_VK_DEVICE_CREATED` evidence (`STANDARD_24M`) with `group_id`, `device_id`, `vram_budget_bytes`.

Subsequent bindings from the same group reuse this `VkDevice`.

### 5.2 When a `VkDevice` is torn down

When all sessions in group A have ended (logout, app termination, recovery exit) and an inactivity grace period of `inactive_group_vk_device_ttl` (default 5 minutes) has elapsed, the L8 service:

1. Revokes all outstanding `GpuCapabilityBinding`s under that `(group_id, device_id)`.
2. Waits for in-flight commands to drain (`vkDeviceWaitIdle` with a 2 s timeout; if the timeout fires, the device is destroyed regardless and a `GPU_DEVICE_FORCE_RECLAIMED` evidence record is emitted).
3. Calls `vkDestroyDevice`.
4. Releases the VRAM budget back to the device's free pool.
5. Emits `GPU_VK_DEVICE_DESTROYED` evidence (`STANDARD_24M`).

### 5.3 Memory isolation guarantee

Two different `VkDevice`s on the same physical GPU receive memory from disjoint allocation pools managed by the kernel driver. With IOMMU enforced, the GPU's MMU maps each `VkDevice`'s memory pages into its own address space; addresses valid in group A's `VkDevice` are not valid in group B's. Without IOMMU (older systems, certain virtualised hosts), the contract degrades: pages are still accounting-disjoint but a malicious driver could in principle bypass the boundary. The L8 service checks IOMMU status at boot via `/sys/kernel/iommu_groups/`; if absent, every `IssueCapabilityBinding` for `GPU_FULL_3D` or `GPU_COMPUTE_HEAVY` emits `IOMMU_UNAVAILABLE_DEGRADED` evidence (`EXTENDED_60M`) and the binding succeeds with a `degraded_isolation = true` flag that consumers (S7.1, S3.2) surface to the operator.

### 5.4 Why per-group `VkDevice` rather than per-process

Per-process would be the most aggressive isolation but would multiply VRAM overhead by the number of processes — a typical desktop session has ~30 GPU-bearing processes, and Vulkan's per-device allocator overhead (~16 MiB) makes 30 devices unaffordable on 8 GiB GPUs. Per-group is the correct unit because S4.1's privacy boundary is the group, not the process: two processes in the same group already trust each other (same operator, same data scope), so they can share a `VkDevice` without breaking the privacy model. Two groups never trust each other (default-deny), so they get separate devices.

## 6. dmabuf brokering

### 6.1 What a dmabuf is

A dmabuf is a Linux kernel file descriptor that names a buffer of GPU memory and may be passed across process boundaries. The classic use case is a Wayland subsurface: a client renders into its own GPU memory and hands the dmabuf to the compositor, which composes it into the screen without copying. dmabuf is the standard zero-copy mechanism for cross-process GPU sharing.

### 6.2 Brokering rules

Every dmabuf produced by an AIOS-managed `VkDevice` carries an L8-issued `DmabufGrant` describing its authorized peer set. A peer that imports the dmabuf checks the grant's signature and the grant's peer list. Imports without a grant, or with a grant whose peer list does not include the importer, fail with `DmabufImportDenied`.

### 6.3 Default peer sets

Per §3.2: the default peer set depends on the source surface's capability class. Same-group, same-subject is always allowed (subject to class). `_system:kwin` is allowed for all classes that produce a render output (i.e., not pure compute). Cross-group is denied.

### 6.4 The `AuthorizeDmabufPeer` RPC

A surface owner may request that an additional peer be authorized for a specific surface's dmabuf. The L8 service evaluates the request:

- If the peer is in the same group: granted (with `GPU_DMABUF_GRANTED` evidence, `STANDARD_24M`).
- If the peer is in a different group: refused with `DmabufCrossGroupForbidden` and `GPU_DMABUF_DENIED` evidence (`STANDARD_24M`). To override, an operator-initiated, policy-approved grant must be explicitly authored; this is the `_system` audit-read exception of S4.1 §9.2 expressed at the GPU layer.
- If the peer is `_system:kwin`: always granted for surfaces with a render output.
- If the peer is `_system:audit-recorder` (a hypothetical evidence-stream recorder under the `_system` scope): granted only when recovery mode is active and `system_audit_read` capability is present on the requesting subject.

### 6.5 Revocation and fence semantics

Each `DmabufGrant` carries a fence counter. The grant may be revoked by the L8 service (e.g., on group teardown, on policy change, on capability binding revocation). Revocation increments the fence counter; subsequent imports against the revoked grant fail with `DmabufRevoked`. Already-imported dmabufs continue to function until the importing process closes them — the kernel does not retroactively unmap; the contract is that **new imports** are blocked, not that **active mappings** are torn down. For high-security tear-down, the operator may use `RevokeCapabilityBinding`, which destroys the underlying `VkDevice` and forces all consumers to fault.

## 7. Capability binding lifecycle

### 7.1 `GpuCapabilityBinding` message

```proto
message GpuCapabilityBinding {
  string binding_id = 1;                         // gpubind_<ulid>
  GpuCapabilityClass capability_class = 2;
  string subject_canonical_id = 3;               // L4 identity
  string group_id = 4;
  string device_id = 5;                          // gpu_<ulid> from §4
  string vk_device_handle = 6;                   // opaque handle for native renderers; "" on Web
  string web_adapter_origin = 7;                 // "https://aios.localhost/<group_id>/" on Web; "" on native
  uint64 vram_budget_bytes = 8;
  QueuePriority queue_priority = 9;
  uint32 frame_rate_cap_fps = 10;
  bool degraded_isolation = 11;                  // §5.3 IOMMU-absent flag
  google.protobuf.Timestamp issued_at = 12;
  google.protobuf.Timestamp expires_at = 13;     // mirrors L4 session expiry (S5.1 §8.2)
  CapabilityBindingState state = 14;
  bytes ed25519_signature = 15;                  // signed by L8 GPU service
}

enum CapabilityBindingState {
  CAPABILITY_BINDING_STATE_UNSPECIFIED = 0;
  REQUESTED = 1;
  ACTIVE = 2;
  EXPIRED = 3;
  REVOKED = 4;
}

enum QueuePriority {
  QUEUE_PRIORITY_UNSPECIFIED = 0;
  NICE = 1;
  NORMAL = 2;
  HIGH = 3;
}
```

### 7.2 Lifecycle

```text
REQUESTED → ACTIVE   (after L8 issues and signs)
ACTIVE    → EXPIRED  (expires_at passed)
ACTIVE    → REVOKED  (operator action, group teardown, policy change, capability lie detected)
```

Forbidden: `EXPIRED → ACTIVE`, `REVOKED → ACTIVE`. Renewal is a fresh binding (new `binding_id`); there is no in-place renewal.

### 7.3 Renewal

A subject whose binding nears expiry may call `IssueCapabilityBinding` again with the same parameters; the new binding is independent. The renewer is responsible for re-attaching surfaces to the new binding. This avoids the implicit-mutation pattern S5.1 forbids for identity records.

### 7.4 Verification by consumers

S7.1's surface service, on `CreateSurface`, calls `IssueCapabilityBinding` with the requested `gpu_capability_class`, attaches the returned `binding_id` to the `Surface` record, and verifies the binding's signature before composition. S3.2's sandbox enforcement verifies the binding's `capability_class` against the sandbox's `GpuPolicy` (cross-spec touch-up queued — see §11) at action apply time and refuses execution if the binding is broader than the sandbox permits.

## 8. Hardware capability lies

### 8.1 What this is

A driver may report support for an API version that the underlying kernel-mode driver does not actually expose. This happens on systems with mismatched user-space and kernel-space driver components, on virtualised hosts whose passthrough is incomplete, and (rarely) under deliberate tampering. Trusting the user-space report at face value would let a process compile shaders that the kernel later refuses, leaking a fault to the application; in adversarial scenarios it would also allow capability inflation.

### 8.2 Detection

At every `IssueCapabilityBinding` call, the L8 service re-probes:

- Vulkan: compare `vkEnumerateInstanceVersion()` (user-space) with `/sys/class/drm/card*/device/driver/version` and the loaded kernel module's reported caps.
- OpenGL: compare `glGetString(GL_VERSION)` with mesa-side `MESA_GL_VERSION_OVERRIDE` policy and the kernel module's caps.
- WebGPU: not applicable — the browser is the trust boundary.

A mismatch where user-space claims more than the kernel exposes is a `HOST_CAPABILITY_LIE`. The binding is refused with `BindingSignatureInvalid` is **not** the right error code (the signature is fine; the underlying capability is fraudulent); the correct code is `HostCapabilityLie`. Evidence is emitted with `FOREVER` retention. This mirrors S3.2 §6.3 capability-lie discipline.

### 8.3 Why FOREVER retention

A capability lie is either a system corruption (rare; needs forensic record) or a tampering attempt (rare; needs forensic record). The volume is tiny by construction; the cost of `FOREVER` is negligible; the benefit is keeping the lie observable across operator-driven log retention cycles.

## 9. Adversarial robustness

### 9.1 VRAM exhaustion attack

A malicious or buggy app requests `GPU_FULL_3D` and allocates 100% of VRAM, denying other workloads. Mitigations:

- Per-`(group, subject, capability_class)` accounting (§I6) caps the attacker at their class budget.
- Per-group budget caps the attacker's whole group at the group floor (default 25% of total VRAM).
- The `_system:kwin` compositor is reserved a hard 256 MiB it cannot lose, so chrome remains visible during attack.
- On allocation failure, the L8 service emits `GPU_BUDGET_EXCEEDED` evidence (`EXTENDED_60M`) and rejects further allocation; it does not OOM-kill the process.

### 9.2 dmabuf reuse-after-revoke

An attacker holds a dmabuf grant across a revocation event. Mitigation: every grant carries a fence counter; new imports against a stale fence fail with `DmabufRevoked`. Pre-revocation imports remain mapped (per kernel semantics) until the importing process exits, but the L8 service can force-destroy the underlying `VkDevice` to break those mappings if the operator escalates.

### 9.3 Capability binding forgery

An attacker fabricates a `GpuCapabilityBinding`. Mitigation: every binding is Ed25519-signed by the L8 service; consumers verify. The L8 signing key is held in the Vault Broker (L4 §X) with capability-only access; the L8 service is the sole subject capable of `gpu.capability.sign`.

### 9.4 Cross-group readback via shader uniform

An app in group A binds a texture from group B as a shader uniform input. Mitigation: the texture handle is local to group A's `VkDevice`; group B's textures are not addressable. With IOMMU enforced, this is hard-blocked at the GPU's MMU. Without IOMMU, the binding's `degraded_isolation` flag is true and the operator-visible warning is on; in this degraded mode, cross-group reads emit `CROSS_SURFACE_READ_DENIED` evidence (`FOREVER`, defined in S7.1 §9) but cannot be perfectly prevented.

### 9.5 Driver bug exploits

Vulkan validation layers catch many driver and application bugs but expand attack surface. AIOS enables them in normal mode (catching bugs is more valuable than the attack-surface increase) and disables them in recovery mode (smaller attack surface for the recovery path; the recovery shell does no novel rendering). Each transition emits `GPU_VALIDATION_DISABLED_RECOVERY` or `GPU_VALIDATION_ENABLED_NORMAL` evidence (`STANDARD_24M`).

### 9.6 Compute side-channels

`GPU_COMPUTE_HEAVY` is the only class with compute pipelines. Side-channel leaks (timing, power) between compute workloads on the same GPU are a known concern. AIOS does not claim to defeat sophisticated side-channel attacks; it does serialise compute submissions across groups (one group's compute queue at a time per GPU) when more than one group requests `GPU_COMPUTE_HEAVY`, which collapses the most obvious leakage. This is a pragmatic mitigation, not a guarantee.

### 9.7 Recovery escape via GPU

A pre-recovery GPU resource holding a render target across the boot transition could in principle leak pre-recovery framebuffers. Mitigation: the L8 service destroys all `VkDevice`s on entry to recovery (recovery boots fresh) and re-creates only what the recovery shell requests. This guarantees recovery starts from a known-clean GPU state.

## 10. Performance contract

| Operation                                            | p50      | p95      | p99      | Hard timeout |
| ---------------------------------------------------- | -------- | -------- | -------- | ------------ |
| `EnumerateDevices` (cold; once at boot)              | < 30 ms  | < 100 ms | < 250 ms | 1 s          |
| `EnumerateDevices` (cache hit)                       | < 100 µs | < 500 µs | < 2 ms   | 50 ms        |
| `IssueCapabilityBinding` (subject already auth'd)    | < 10 ms  | < 50 ms  | < 200 ms | 1 s          |
| VRAM allocation per surface (Vulkan, native)         | < 1 ms   | < 10 ms  | < 50 ms  | 200 ms       |
| dmabuf handoff (fd passing)                          | < 100 µs | < 1 ms   | < 5 ms   | 50 ms        |
| `AuthorizeDmabufPeer`                                | < 5 ms   | < 25 ms  | < 100 ms | 500 ms       |
| `RevokeCapabilityBinding` (binding only)             | < 5 ms   | < 50 ms  | < 200 ms | 1 s          |
| `RevokeCapabilityBinding` (incl. `VkDevice` destroy) | < 100 ms | < 1 s    | < 5 s    | 10 s         |

Failure modes — all fail closed:

- `GpuExhausted` — no GPU has the requested VRAM; binding refused; `GPU_BUDGET_EXCEEDED` evidence.
- `DriverUnavailable` — no usable GPU driver loaded; binding refused; `DRIVER_UNAVAILABLE` evidence (`STANDARD_24M`).
- `IommuUnavailable` — IOMMU off; binding succeeds with `degraded_isolation = true` for `GPU_FULL_3D`/`GPU_COMPUTE_HEAVY`; `IOMMU_UNAVAILABLE_DEGRADED` evidence.
- `CapabilityClassDenied` — sandbox / policy denies the requested class; binding refused; `GPU_CAPABILITY_DENIED` evidence (`STANDARD_24M`).
- `DmabufCrossGroupForbidden` — peer is in a different group; grant refused; `GPU_DMABUF_DENIED` evidence.
- `BindingSignatureInvalid` — consumer presented a forged or corrupted binding; consumer rejects; `GPU_BINDING_FORGERY` evidence (`FOREVER`).
- `HostCapabilityLie` — driver/kernel mismatch; binding refused; `HOST_CAPABILITY_LIE` evidence (`FOREVER`).

## 11. Cross-spec dependencies

| Spec                         | Direction | What this spec contributes / consumes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ---------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| S0.1                         | producer  | Action envelopes whose targets need GPU resources may carry a hint `target.gpu_capability_class`; the Capability Runtime can pre-issue a binding before dispatch                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| S1.3                         | consumer  | `GpuCapabilityBinding` and `DmabufGrant` are bound to surface objects whose `ScopeBinding` (S1.3 §21.1) provides `group_id` for the per-group `VkDevice` selection                                                                                                                                                                                                                                                                                                                                                                                                                               |
| S2.1                         | producer  | New closed query field `target.gpu_capability_class` (queued for follow-up touch-up)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| S2.3                         | producer  | New closed condition fields `target.gpu_capability_class`, `target.gpu_device_kind` (`integrated`/`discrete`); constitutional hard-deny `GpuComputeOutsideAuthorisedClass` candidate (NOT applied here; flagged for follow-up)                                                                                                                                                                                                                                                                                                                                                                   |
| S2.4                         | producer  | New primitive `gpu_binding_class(binding_id, expected_class)` for property checks (queued)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| S3.1                         | producer  | Eight new record types: `GPU_DEVICE_ENUMERATED`, `GPU_DEVICE_DISCONNECTED`, `GPU_VK_DEVICE_CREATED`, `GPU_VK_DEVICE_DESTROYED` (all `STANDARD_24M`); `GPU_BUDGET_EXCEEDED`, `GPU_BUDGET_DOWNGRADED` (`EXTENDED_60M`); `GPU_DMABUF_GRANTED`, `GPU_DMABUF_DENIED` (`STANDARD_24M`); `GPU_CAPABILITY_DENIED`, `IOMMU_UNAVAILABLE_DEGRADED` (`STANDARD_24M` and `EXTENDED_60M` respectively); `GPU_VALIDATION_DISABLED_RECOVERY`, `GPU_VALIDATION_ENABLED_NORMAL`, `DRIVER_UNAVAILABLE` (`STANDARD_24M`); `HOST_CAPABILITY_LIE`, `GPU_BINDING_FORGERY`, `GPU_DEVICE_FORCE_RECLAIMED` (all `FOREVER`) |
| S3.2                         | producer  | New field `gpu_policy: GpuPolicy` on `SandboxProfile`, where `GpuPolicy { max_capability_class, allowed_devices, deny_compute }` constrains the binding the sandbox will tolerate; cross-spec touch-up queued                                                                                                                                                                                                                                                                                                                                                                                    |
| S4.1                         | consumer  | `group_id` from S4.1 §7.1 is the partitioning key; `_system` scope is the privileged peer set (`_system:kwin`, `_system:audit-recorder`)                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| S5.1                         | consumer  | Subject canonical id and `primary_group_id` from S5.1 are bound into every `GpuCapabilityBinding`; `recovery_mode` flag drives §I10 validation behaviour                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| S7.1                         | consumer  | `Surface.gpu_capability_class` references this spec's enum; `Surface.gpu_capability_binding_id` references this spec's `GpuCapabilityBinding`                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| L0 INV-007 (downward deps)   | enforcer  | This spec lives in L8 (below L7); S7.1 in L7 consumes it correctly; no upward dependency                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| L7.4 KDE renderer (deferred) | consumer  | Implements `wgpu` over Vulkan with the per-group `VkDevice` partitioning of §5; KWin acts as `_system:kwin` privileged dmabuf peer                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| L7.5 Web renderer (deferred) | consumer  | Implements `wgpu` over WebGPU with per-origin `GPUAdapter` sandboxing; iframe origin = `https://aios.localhost/<group_id>/`                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |

### 11.1 Cross-spec touch-ups queued

The following cannot be applied within this spec without violating the discipline that one spec owns one contract; they are flagged for the next consolidation cycle:

- **S2.3** to add closed condition fields `target.gpu_capability_class`, `target.gpu_device_kind` and a constitutional hard-deny `GpuComputeOutsideAuthorisedClass`.
- **S2.4** to add primitive `gpu_binding_class(binding_id, expected_class)`.
- **S3.1** to absorb the 16 new record types listed above into the closed `RecordType` enum and the retention-class table.
- **S3.2** to add `gpu_policy: GpuPolicy` field on `SandboxProfile` and the apply-time check that the issued binding's class is `≤` the sandbox's `max_capability_class`.
- **S2.1** to add closed query field `target.gpu_capability_class`.

## 12. Golden fixtures

### Fixture 1 — Single-group GPU usage (family evidence viewer)

```text
Setup:
  Subject: family:alice, primary_group_id = family
  Device: gpu_01 (DISCRETE, NVIDIA RTX 4070, 12 GiB VRAM, IOMMU enforced)

IssueCapabilityBindingRequest {
  capability_class: GPU_BASIC_2D,
  subject_canonical_id: "family:alice",
  group_id: "family",
  preferred_device_id: ""              // any
}

Expected:
  L8 selects gpu_00 (INTEGRATED Intel iGPU) for GPU_BASIC_2D.
  Creates VkDevice for ("family", gpu_00) with 64 MiB budget.
  Returns GpuCapabilityBinding {
    binding_id: "gpubind_<ulid>",
    capability_class: GPU_BASIC_2D,
    device_id: "gpu_00",
    vram_budget_bytes: 67108864,
    queue_priority: NICE,
    frame_rate_cap_fps: 60,
    degraded_isolation: false,
    state: ACTIVE,
    ed25519_signature: <bytes>
  }
  Evidence: GPU_VK_DEVICE_CREATED (first time for family on gpu_00).
```

### Fixture 2 — Cross-group `VkDevice` isolation

```text
Setup:
  Subject A: family:alice (primary_group_id = family)
  Subject B: homelab:bob (primary_group_id = homelab)
  Device: gpu_01 (16 GiB VRAM, IOMMU enforced)
  Both run GPU_RICH_2D surfaces concurrently.

Expected:
  L8 issues two distinct VkDevices:
    - vk_dev_family on gpu_01, 25% budget (4 GiB)
    - vk_dev_homelab on gpu_01, 25% budget (4 GiB)
  Each surface's VRAM allocations come from its group's VkDevice.
  Memory isolation: addresses in vk_dev_family are not valid in vk_dev_homelab.
  No shared memory; cross-group readback impossible.
  Evidence: two GPU_VK_DEVICE_CREATED records, one per group.
```

### Fixture 3 — Cross-group dmabuf denial

```text
Setup:
  Surface S_a in group "family", capability_class GPU_RICH_2D, owned by family:alice.
  S_a produced a dmabuf handle for an internal share.
  An app in group "homelab" calls AuthorizeDmabufPeer attempting to import S_a's dmabuf.

Expected:
  L8 evaluates: requesting peer's group ("homelab") ≠ source surface's group ("family").
  Refuses with DmabufCrossGroupForbidden.
  Emits GPU_DMABUF_DENIED evidence (STANDARD_24M) carrying:
    source_group: "family"
    target_group: "homelab"
    source_subject: "family:alice"
    target_subject: "homelab:app:com.example.tool:i-01"
  Importing process receives error; texture is not accessible.
```

### Fixture 4 — VRAM exhaustion fallback (downgrade)

```text
Setup:
  Subject: family:alice
  Device gpu_01: 8 GiB total, currently 7.6 GiB allocated (95%).
  IssueCapabilityBindingRequest { capability_class: GPU_FULL_3D, ... }

Expected:
  L8 computes: GPU_FULL_3D would need ≥ 256 MiB minimum on this device; only 400 MiB free,
  but family group's per-group share is already used.
  L8 evaluates downgrade path: GPU_RICH_2D requires only 64 MiB minimum; available.
  L8 issues GpuCapabilityBinding { capability_class: GPU_RICH_2D, ... } AND emits
  GPU_BUDGET_DOWNGRADED evidence (EXTENDED_60M):
    requested_class: GPU_FULL_3D
    issued_class: GPU_RICH_2D
    reason: "group_vram_budget_insufficient_for_requested_class"
  Caller receives the downgraded binding. The downgrade is explicit, not silent.
```

### Fixture 5 — Hardware capability lie detected

```text
Setup:
  System has NVIDIA driver claiming Vulkan 1.3 but kernel module exposes only Vulkan 1.1
  (e.g., user-space driver upgraded, kernel module not reloaded).

IssueCapabilityBindingRequest { capability_class: GPU_FULL_3D, ... }

L8 re-probe:
  vkEnumerateInstanceVersion() → 1.3.250
  /sys/class/drm/card0/device/driver/version → kernel module 1.1.x
  Mismatch: user-space claims more than kernel exposes.

Expected:
  L8 refuses binding with HostCapabilityLie.
  Emits HOST_CAPABILITY_LIE evidence (FOREVER) carrying:
    device_id: "gpu_01"
    user_space_version: "1.3.250"
    kernel_exposed_version: "1.1.x"
    detected_at: <timestamp>
  Caller receives error; no binding issued.
  Operator alert raised (S9.X observability hook).
```

### Fixture 6 — Compositor privileged dmabuf access

```text
Setup:
  Surfaces S_a (group "family"), S_b (group "homelab"), S_c (group "_system") all active.
  Compositor _system:kwin needs dmabufs from all three to compose the screen.

Expected:
  For each surface, kwin calls AuthorizeDmabufPeer(target = "_system:kwin").
  L8 evaluates: peer is "_system:kwin", which is the privileged compositor in §I11.
  All three grants succeed regardless of source group.
  Emits three GPU_DMABUF_GRANTED records (STANDARD_24M).
  KWin composes opaquely: it uses each dmabuf as a render input but does not copy
  pixels into another group's VkDevice. Cross-group pixel reads remain impossible.
```

### Fixture 7 — Recovery-mode GPU validation disabled

```text
Setup:
  Boot enters recovery mode; subject = _system:local:operator-247, recovery_mode = true.
  Recovery shell needs minimal GPU for status display.

IssueCapabilityBindingRequest { capability_class: GPU_PASSIVE_DISPLAY, ... }

Expected:
  L8 detects recovery flag.
  Creates VkDevice WITHOUT Vulkan validation layers (smaller attack surface).
  Emits GPU_VALIDATION_DISABLED_RECOVERY evidence (STANDARD_24M).
  Returns GpuCapabilityBinding with vram_budget_bytes = 16 MiB, queue_priority = NICE.
  Recovery shell renders.
  On exit from recovery, validation layers re-enabled; GPU_VALIDATION_ENABLED_NORMAL evidence.
```

### Fixture 8 — Multi-GPU routing

```text
Setup:
  System: gpu_00 (INTEGRATED Intel iGPU, 1 GiB shared), gpu_01 (DISCRETE NVIDIA, 16 GiB)
  Subject A: family:alice issues GPU_PASSIVE_DISPLAY (a clock widget).
  Subject B: family:alice issues GPU_COMPUTE_HEAVY (an ML inference job).

Expected:
  Request A → routed to gpu_00 (INTEGRATED preferred for PASSIVE/BASIC_2D).
              VkDevice on gpu_00 for family group created.
  Request B → routed to gpu_01 (DISCRETE preferred for COMPUTE_HEAVY).
              VkDevice on gpu_01 for family group created.
  Both succeed concurrently. Evidence: two GPU_VK_DEVICE_CREATED records.
  The two VkDevices are independently accounted; gpu_00 and gpu_01 budgets do not interact.
```

## 13. Telemetry contract

All metrics MUST use bounded label cardinality. **`subject_canonical_id`, `group_id`, `binding_id`, `vk_device_handle`, `device_id` are NEVER labels.** `gpu_kind` and `vendor` are bounded (≤ 6 vendor values across the realistic GPU vendor space).

| Metric                                    | Type      | Labels (closed)                                                         |
| ----------------------------------------- | --------- | ----------------------------------------------------------------------- |
| `gpu_capability_binding_issue_total`      | counter   | `capability_class`, `gpu_kind`, `result` (success/error), `error_code`  |
| `gpu_capability_binding_active`           | gauge     | `capability_class`, `gpu_kind`                                          |
| `gpu_capability_binding_duration_seconds` | histogram | `capability_class`                                                      |
| `gpu_capability_binding_revoke_total`     | counter   | `reason_class` (expired/operator/group_teardown/lie/policy)             |
| `gpu_vk_device_active`                    | gauge     | `gpu_kind`                                                              |
| `gpu_vk_device_create_total`              | counter   | `gpu_kind`                                                              |
| `gpu_vk_device_destroy_total`             | counter   | `reason_class` (idle/teardown/force/recovery)                           |
| `gpu_vram_allocated_bytes`                | gauge     | `gpu_kind`, `capability_class`                                          |
| `gpu_vram_total_bytes`                    | gauge     | `gpu_kind`                                                              |
| `gpu_dmabuf_grant_total`                  | counter   | `result` (granted/denied), `peer_class` (system/same_group/cross_group) |
| `gpu_capability_lie_detected_total`       | counter   | none                                                                    |
| `gpu_iommu_unavailable_total`             | counter   | none                                                                    |
| `gpu_validation_state`                    | gauge     | `state` (enabled/disabled)                                              |
| `gpu_budget_exceeded_total`               | counter   | `capability_class`                                                      |
| `gpu_budget_downgraded_total`             | counter   | `requested_class`, `issued_class`                                       |

Cardinality budget: ≤ 200 active label tuples per metric. With 5 capability classes × 2 `gpu_kind` × ≤ 12 error codes the worst case is 120 — within budget.

## 14. Acceptance criteria

- [ ] `GpuCapabilityClass` is a closed enum with five values plus the `_UNSPECIFIED` zero; class budgets match §3.1.
- [ ] `QueuePriority` is a closed enum with three values (`NICE`, `NORMAL`, `HIGH`).
- [ ] `GpuKind` is a closed enum with two values (`INTEGRATED`, `DISCRETE`).
- [ ] `CapabilityBindingState` lifecycle has four states with the exact transitions in §7.2; forbidden transitions rejected.
- [ ] Per-group `VkDevice` partitioning protocol of §5 implemented; idle teardown after `inactive_group_vk_device_ttl` (default 5 min).
- [ ] Per-origin `GPUAdapter` sandboxing on Web verified by iframe-origin equality with `ScopeBinding.group_id`.
- [ ] dmabuf cross-group passing denied by default; `_system:kwin` is the only constitutional privileged peer; `AuthorizeDmabufPeer` recorded as evidence.
- [ ] Capability binding signed; consumers (S7.1 surface service, S3.2 sandbox enforcement) verify before use.
- [ ] Hardware capability lies detected at every binding request; `HOST_CAPABILITY_LIE` evidence is `FOREVER`-retained.
- [ ] IOMMU absence surfaces as `degraded_isolation = true` on the binding plus `IOMMU_UNAVAILABLE_DEGRADED` evidence; bindings still succeed.
- [ ] Recovery mode disables Vulkan validation; entry/exit emit evidence.
- [ ] All eight golden fixtures (§12) produce the specified outcomes.
- [ ] Telemetry conforms to §13; subject / group / binding / device ids never appear as labels.
- [ ] L0 INV-007 layer-downward dependency satisfied: this spec depends only on L0–L7 contracts (S0.1, S1.3, S2.3, S3.2, S4.1, S5.1, S7.1) — no upward dependency.

## 15. Open deferrals

- **GPU partitioning via SR-IOV / MIG** — NVIDIA Multi-Instance GPU and AMD SR-IOV enable hardware-level partitioning that strengthens isolation beyond per-`VkDevice`. Deferred to a future spec when the cost/benefit on small-footprint hardware is clearer.
- **GPU thermal / power budgeting** — per-class TDP ceilings to prevent a `GPU_COMPUTE_HEAVY` job from thermal-throttling a concurrent `GPU_FULL_3D` game. Deferred.
- **External GPU (eGPU) hot-plug** — Thunderbolt/USB4-attached GPUs that may appear and disappear. Mechanics deferred; the enumeration protocol of §4.2 is hot-plug-aware in principle but the binding migration story is open.
- **Cross-machine GPU sharing (CUDA-over-RDMA, etc.)** — using a remote GPU from a homelab node. Deferred to L8 network contracts and a future distributed compute spec.
- **GPU-accelerated codecs (NVENC, VAAPI) per-class quotas** — encoder slot accounting separate from VRAM. Deferred.
- **Tile-based GPU resident-set caps for ARM SoCs** — many ARM GPUs use tile-based deferred rendering; budgets in tile space differ from VRAM space. Deferred until ARM AIOS targets exist.
- **Per-class shader allow-lists with cryptographic signing** — pinning shader binaries to a class signature to defeat shader-level capability lies. Deferred.
- **GPU snapshot/restore for live migration** — capturing a `VkDevice` state and restoring it on another host. Deferred.
- **Operator-configurable per-group VRAM budgets via policy bundles** — the §3.1 budgets are this spec's floors; the override mechanism is queued for the S2.3 / policy-bundle touch-up.

## 16. See also

- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S1.3 — AIOS-FS Object Model](../L2_AIOS_FS/01_object_model.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S3.2 — Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [S4.1 — Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S5.1 — Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [S7.1 — Surface + Composition Model](../L7_Interaction_Renderers/01_surface_composition.md)
- [L0 INV-007 layer downward dependency](../L0_Governance_Evidence_Safety/04_invariants.md)
- [L8 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

## Appendix A — Full Proto IDL

```proto
syntax = "proto3";
package aios.gpu.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/duration.proto";

// ============================================================================
// Service
// ============================================================================

service GpuResourceService {
  rpc EnumerateDevices(EnumerateDevicesRequest) returns (EnumerateDevicesResponse);
  rpc IssueCapabilityBinding(IssueCapabilityBindingRequest) returns (IssueCapabilityBindingResponse);
  rpc RevokeCapabilityBinding(RevokeCapabilityBindingRequest) returns (RevokeCapabilityBindingResponse);
  rpc AuthorizeDmabufPeer(AuthorizeDmabufPeerRequest) returns (AuthorizeDmabufPeerResponse);
  rpc GetGpuResourceInfo(GetGpuResourceInfoRequest) returns (GetGpuResourceInfoResponse);
}

// ============================================================================
// Closed enums
// ============================================================================

enum GpuCapabilityClass {
  GPU_CAPABILITY_CLASS_UNSPECIFIED = 0;
  GPU_PASSIVE_DISPLAY = 1;
  GPU_BASIC_2D = 2;
  GPU_RICH_2D = 3;
  GPU_FULL_3D = 4;
  GPU_COMPUTE_HEAVY = 5;
}

enum GpuKind {
  GPU_KIND_UNSPECIFIED = 0;
  INTEGRATED = 1;
  DISCRETE = 2;
}

enum QueuePriority {
  QUEUE_PRIORITY_UNSPECIFIED = 0;
  NICE = 1;
  NORMAL = 2;
  HIGH = 3;
}

enum CapabilityBindingState {
  CAPABILITY_BINDING_STATE_UNSPECIFIED = 0;
  REQUESTED = 1;
  ACTIVE = 2;
  EXPIRED = 3;
  REVOKED = 4;
}

enum IssueCapabilityBindingErrorCode {
  ISSUE_CAPABILITY_BINDING_ERROR_CODE_UNSPECIFIED = 0;
  GPU_EXHAUSTED = 1;
  DRIVER_UNAVAILABLE = 2;
  CAPABILITY_CLASS_DENIED = 3;
  HOST_CAPABILITY_LIE = 4;
  SUBJECT_NOT_RESOLVED = 5;
  GROUP_INACTIVE = 6;
  PREFERRED_DEVICE_UNAVAILABLE = 7;
  POLICY_REFUSED = 8;
}

enum AuthorizeDmabufPeerErrorCode {
  AUTHORIZE_DMABUF_PEER_ERROR_CODE_UNSPECIFIED = 0;
  DMABUF_CROSS_GROUP_FORBIDDEN = 1;
  DMABUF_BINDING_INVALID = 2;
  DMABUF_PEER_UNRESOLVED = 3;
  DMABUF_REVOKED = 4;
}

// ============================================================================
// Core types
// ============================================================================

message GpuApiSupport {
  string vulkan_version = 1;
  string opengl_version = 2;
  string opencl_version = 3;
  string metal_version = 4;
  string dx12_version = 5;
  bool webgpu_supported = 6;
  bool ray_tracing_supported = 7;
  bool mesh_shader_supported = 8;
}

message GpuDevice {
  string device_id = 1;
  GpuKind kind = 2;
  string vendor = 3;
  string model = 4;
  string driver_name = 5;
  string driver_version = 6;
  uint64 vram_total_bytes = 7;
  uint64 vram_currently_allocated_bytes = 8;
  GpuApiSupport api_support = 9;
  bool iommu_enforced = 10;
  google.protobuf.Timestamp first_seen_at = 11;
  google.protobuf.Timestamp last_health_check_at = 12;
}

message GpuCapabilityBinding {
  string binding_id = 1;
  GpuCapabilityClass capability_class = 2;
  string subject_canonical_id = 3;
  string group_id = 4;
  string device_id = 5;
  string vk_device_handle = 6;
  string web_adapter_origin = 7;
  uint64 vram_budget_bytes = 8;
  QueuePriority queue_priority = 9;
  uint32 frame_rate_cap_fps = 10;
  bool degraded_isolation = 11;
  google.protobuf.Timestamp issued_at = 12;
  google.protobuf.Timestamp expires_at = 13;
  CapabilityBindingState state = 14;
  bytes ed25519_signature = 15;
}

message DmabufGrant {
  string grant_id = 1;                           // dmabuf_<ulid>
  string source_binding_id = 2;                  // gpubind_<ulid>
  repeated string authorized_peer_subject_ids = 3;
  uint64 fence_counter = 4;                      // increments on revoke
  google.protobuf.Timestamp issued_at = 5;
  google.protobuf.Timestamp expires_at = 6;
  bytes ed25519_signature = 7;
}

// ============================================================================
// RPC request/response
// ============================================================================

message EnumerateDevicesRequest {
  bool force_refresh = 1;                        // bypass cache
}
message EnumerateDevicesResponse {
  repeated GpuDevice devices = 1;
  google.protobuf.Timestamp enumerated_at = 2;
}

message IssueCapabilityBindingRequest {
  GpuCapabilityClass capability_class = 1;
  string subject_canonical_id = 2;
  string group_id = 3;
  string preferred_device_id = 4;                // optional; "" = any
  bool allow_downgrade = 5;                      // default true
  google.protobuf.Duration requested_ttl = 6;    // capped at session TTL (S5.1)
}
message IssueCapabilityBindingResponse {
  oneof result {
    GpuCapabilityBinding binding = 1;
    IssueCapabilityBindingError error = 2;
  }
}
message IssueCapabilityBindingError {
  IssueCapabilityBindingErrorCode code = 1;
  string message = 2;
  GpuCapabilityClass downgrade_offered = 3;      // if non-UNSPECIFIED, caller may retry
}

message RevokeCapabilityBindingRequest {
  string binding_id = 1;
  string reason = 2;                             // "operator" | "expiry" | "policy" | "lie"
  bool destroy_underlying_vk_device = 3;         // forces device-level teardown
}
message RevokeCapabilityBindingResponse {
  bool revoked = 1;
}

message AuthorizeDmabufPeerRequest {
  string source_binding_id = 1;
  string target_subject_canonical_id = 2;
  google.protobuf.Duration ttl = 3;
}
message AuthorizeDmabufPeerResponse {
  oneof result {
    DmabufGrant grant = 1;
    AuthorizeDmabufPeerError error = 2;
  }
}
message AuthorizeDmabufPeerError {
  AuthorizeDmabufPeerErrorCode code = 1;
  string message = 2;
}

message GetGpuResourceInfoRequest {}
message GetGpuResourceInfoResponse {
  string schema_version = 1;                     // "aios.gpu.v1alpha1"
  uint32 active_devices = 2;
  uint32 active_vk_devices = 3;
  uint32 active_capability_bindings = 4;
  uint32 active_dmabuf_grants = 5;
  bool iommu_enforced_globally = 6;
  bool validation_layers_enabled = 7;
  google.protobuf.Duration inactive_group_vk_device_ttl = 8;
}
```
