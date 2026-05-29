# S18 - Kernel Personality and Portability Plane

| Field     | Value                                                                                                                                                                                                                                                                                                            |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-28; awaiting implementation evidence)                                                                                                                                                                                                                                                |
| Phase tag | S18                                                                                                                                                                                                                                                                                                              |
| Layer     | Cross-cutting: L1, L2, L3, L4, L6, L8, L9, L10                                                                                                                                                                                                                                                                   |
| Consumes  | S2.3 Policy Kernel, S3.2 Sandbox Composition, S3.1 Evidence Log, S8.1 Network Policy, S8.2 GPU/Video Policy, S8.3 Hardware Graph, S9.1 Recovery Boundary, S12.2 Package Model, S16.1 Security Profile Matrix, S16.4 Measured Boot + Runtime Integrity                                                            |
| Produces  | `KernelPersonality`, `KernelTargetRegistry`, `KernelCapabilityMatrix`, `KernelBackendAdapter`, `KernelBuildCandidate`, `RTWorkloadManifest`, `RTAdmissionController`, `RTLatencyEvidence`, `RTDeviceBinding`, `RTTeardownPlan`, RTOS sidecar broker contract, kernel portability gates, backend evidence records |

## 1. Responsibility

S18 defines how AIOS can support more than one kernel family without turning the
system into a collection of unrelated operating systems.

The primary implementation target remains Linux. The architecture must keep
kernel-dependent mechanisms behind explicit backend adapters so that FreeBSD,
OpenBSD, NetBSD, PREEMPT_RT, RTOS sidecars, microkernel research targets, and
VM-hosted kernels can be added without rewriting the Policy Kernel, Evidence
Log, App Capsule model, package model, or UI.

Invariant links: INV-002, INV-004, INV-007, INV-008, INV-014, INV-017,
INV-024, INV-028 (boot-integrity authority for kernel candidates and
adapters, see §4 and §9).

> **Document form (per DEC-R3-008).** S18 is an intentional single-file
> contract overview. It is not decomposed into numbered sub-specs; missing
> schemas (`KernelBuildCandidate`, `RTWorkloadManifest`, and the RTOS sidecar
> broker contract) are added in place within this file.

## 2. Product principle

AIOS must not claim that a Linux installation can swap its kernel to BSD and
keep identical driver, package, GPU, container, and desktop behavior.

AIOS may claim kernel portability only at the control-plane level:

```text
AIOS policy/evidence/capsule intent
  -> KernelCapabilityMatrix
  -> selected KernelBackendAdapter
  -> host-kernel enforcement primitive
  -> evidence
```

Linux is the gold path for workstation, gaming, GPU/video, container, and broad
hardware support. BSD personalities are valuable for hardened server, storage,
firewall, appliance, research, and alternative-security profiles.

## 3. Need-driven adaptation rule

Kernel adaptation is not a goal by itself. AIOS changes kernel personality,
kernel config, modules, boot parameters, or backend bindings only when a
declared workload or security profile has a measurable reason.

Valid triggers:

| Trigger                   | Example                                                                                                                       |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `HARDWARE_ENABLEMENT`     | GPU, NIC, storage controller, audio, sensor, or accelerator requires a different driver/module path.                          |
| `SECURITY_POSTURE`        | `STIG_ALIGNED`, `AIRGAP_HIGH`, lockdown, signed modules, IMA/EVM, or MAC policy needs a stricter kernel profile.              |
| `REALTIME_LATENCY`        | Audio, robotics, industrial control, or acquisition workload needs bounded latency.                                           |
| `WORKLOAD_COMPATIBILITY`  | App capsule requires KVM, bhyve, Linuxulator, jails, gVisor, WASI, or microVM support.                                        |
| `PERFORMANCE_CLASS`       | Gaming, AI GPU workstation, video encode/decode, storage appliance, or low-power mobile profile benefits from a tuned kernel. |
| `RECOVERY_OR_RELIABILITY` | Current kernel fails health checks, hardware probes, or boot evidence gates.                                                  |

Invalid triggers:

- novelty
- benchmark chasing without a declared workload
- AI preference without policy request
- replacing a working kernel only because another kernel exists
- weakening a security profile to improve convenience

The operator-facing mechanism must stay simple:

```text
requested goal
  -> show expected benefit
  -> show risk and rollback
  -> build/test candidate away from the active system
  -> canary boot
  -> promote only with evidence
```

## 4. Adaptive kernel forge

AIOS may build a task-specific kernel candidate from a safe generic Linux
bootstrap environment. The generic kernel remains the recovery anchor.

```text
Generic Linux bootstrap kernel
  -> hardware graph probe
  -> workload/profile requirement check
  -> AI-assisted config proposal
  -> isolated kernel build
  -> simulated boot and userspace contract test
  -> signed kernel candidate
  -> canary boot on real hardware
  -> promote or rollback with evidence
```

Candidate profiles:

```text
KernelBuildProfile =
  GENERIC_SAFE
| WORKSTATION_BALANCED
| GAMING_LOW_LATENCY
| AI_GPU_WORKSTATION
| HARDENED_STIG
| SERVER_STORAGE
| PREEMPT_RT
| MOBILE_POWER_SAVER
| APPLIANCE_LOCKED_DOWN
```

Unknown values are rejected by the kernel forge, the candidate promotion gate,
and the recovery UI.

### 4.1 KernelBuildCandidate (core §5 state object)

A kernel candidate produced by the forge is a closed, signed, evidence-backed
state object. It is the unit the promotion gate (§4.2) operates on. No field is
optional; an absent or unknown value is a `FAILED` candidate, not a permissive
default.

```text
KernelBuildCandidate =
  candidate_id                        # "kbc_<ULID>"
  build_profile                       # one KernelBuildProfile value
  base_kernel                         # { family, version, source_ref } of the generic anchor
  config_delta                        # signed, reviewable diff over the base kernel config
  source_and_toolchain_provenance     # source tree hash + toolchain id/hash + SBOM ref (S16.6)
  hermeticity_guarantee              # HERMETIC | NON_HERMETIC; NON_HERMETIC cannot be promoted
  signature_chain                     # operator/trust-root signatures; AI signature is rejected (INV-028)
  simulation_result                   # SimulationResult (see below) or null until simulated
  canary_result                       # CanaryResult (see below) or null until canary-booted
  rollback_target                     # KernelPersonality + boot entry that the system reverts to
  promotion_state                     # PROPOSED | BUILT | SIMULATED | CANARY_PASSED
                                      #   | PROMOTED | ROLLED_BACK | DISCARDED
```

```text
SimulationResult =
  status: PASSED | FAILED | NOT_RUN
  userspace_contract_tests: PASSED | FAILED   # boot + core userspace contract suite
  evidence_receipt_id

CanaryResult =
  status: PASSED | FAILED | NOT_RUN
  real_hardware_boot: PASSED | FAILED
  probe_parity_vs_generic: PASSED | DEGRADED | FAILED   # no silent capability loss
  latency_or_workload_target_met: PASSED | FAILED | NOT_APPLICABLE
  evidence_receipt_id
```

Unknown values for `hermeticity_guarantee`, `promotion_state`, or any `status`
field are rejected by the kernel forge and the promotion gate.

AI agent role:

- propose kernel config deltas
- map hardware graph facts to driver/module choices
- propose boot parameters and module policy
- generate simulation and canary test plans
- explain expected benefit and risk

AI agent hard limits:

- cannot sign or promote a kernel candidate
- cannot disable recovery kernel retention
- cannot weaken the active security profile
- cannot load unsigned modules under stricter profiles
- cannot hide failed boot, probe, latency, or security evidence

### 4.2 Promotion gate (concrete predicates)

The gate operates on a single `KernelBuildCandidate`. Each clause is a concrete,
checkable predicate over candidate fields; all must hold for promotion. The gate
is fail-closed: any clause that is unknown, null, or unverifiable counts as
`false`.

```text
PROMOTE(candidate) ==
      candidate.promotion_state == CANARY_PASSED
  AND candidate.hermeticity_guarantee == HERMETIC
  AND simulation_passes(candidate)
  AND canary_passes(candidate)
  AND rollback_verified(candidate)
  AND benefit_over_generic(candidate)
  AND policy_approval_exists(candidate)        # S2.3 decision on this candidate_id
  AND signature_chain_is_operator_or_trust_root(candidate)   # INV-028: no AI signature
  AND evidence_emitted(candidate)              # KERNEL_CANDIDATE_* chain complete
```

Predicate definitions:

```text
simulation_passes(c) ==
      c.simulation_result.status == PASSED
  AND c.simulation_result.userspace_contract_tests == PASSED

canary_passes(c) ==
      c.canary_result.status == PASSED
  AND c.canary_result.real_hardware_boot == PASSED
  AND c.canary_result.probe_parity_vs_generic in { PASSED, DEGRADED-with-operator-ack }
  AND c.canary_result.latency_or_workload_target_met in { PASSED, NOT_APPLICABLE }

rollback_verified(c) ==
      c.rollback_target names a retained, bootable generic/recovery kernel
  AND a test revert to c.rollback_target booted successfully and emitted
      KERNEL_CANDIDATE_ROLLED_BACK during canary

benefit_over_generic(c) ==
      the declared §3 trigger's measured benefit on c exceeds the generic
      kernel baseline for the same metric (latency, hardware enablement, or
      security-control coverage), recorded in the canary evidence
```

If any predicate is `false`, the candidate stays `BUILT`/`SIMULATED`/
`CANARY_PASSED` (staged) or moves to `DISCARDED`. The system continues on the
generic/recovery kernel. AI subjects can never satisfy
`signature_chain_is_operator_or_trust_root` or `policy_approval_exists` on their
own (INV-028 boot-integrity authority; INV-002 propose-not-execute).

Minimum fields for the `KERNEL_CANDIDATE_*` evidence records:

```text
candidate_id
build_profile
base_kernel
config_delta_hash
source_and_toolchain_provenance_ref
hermeticity_guarantee
signature_chain_ref
promotion_state                 # state at the moment the record was emitted
gate_predicate_results          # map: predicate name -> PASSED | FAILED | NOT_RUN
rollback_target
policy_decision_id              # null for purely informational records, required at PROMOTED
evidence_receipt_id
```

## 5. Reference patterns

| Pattern                                                                                                                                                                                                                   | S18 use                                                                  |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| [FreeBSD Linux binary compatibility](https://docs.freebsd.org/en/books/handbook/book/)                                                                                                                                    | Linux application compatibility on a BSD host, with explicit limits.     |
| [FreeBSD Jails](https://docs.freebsd.org/en/books/handbook/jails/)                                                                                                                                                        | FreeBSD-native OS-level isolation backend.                               |
| [FreeBSD Security / Capsicum / MAC](https://docs.freebsd.org/en/books/handbook/security/)                                                                                                                                 | BSD security primitives for backend mapping.                             |
| [FreeBSD bhyve](https://docs.freebsd.org/en/books/handbook/virtualization/)                                                                                                                                               | VM backend for Linux, BSD, Windows, and appliance capsules.              |
| [OpenBSD pledge(2)](https://man.openbsd.org/pledge.2)                                                                                                                                                                     | Process promise model for OpenBSD research backend.                      |
| [OpenBSD unveil(2)](https://man.openbsd.org/unveil.2)                                                                                                                                                                     | Filesystem visibility restriction model for OpenBSD research backend.    |
| [NetBSD rump kernels](https://www.netbsd.org/docs/rump/sysproxy.html)                                                                                                                                                     | Userspace kernel-component isolation and research backend.               |
| [Linux PREEMPT_RT documentation](https://docs.kernel.org/core-api/real-time/theory.html)                                                                                                                                  | First practical real-time Linux personality.                             |
| [seL4](https://sel4.org/Verification/)                                                                                                                                                                                    | High-assurance microkernel research target, not a desktop baseline.      |
| [illumos](https://illumos.github.io/docs/about/) / [SmartOS](https://docs.smartos.org/)                                                                                                                                   | ZFS, zones, DTrace, and hypervisor-appliance target.                     |
| [gVisor](https://gvisor.dev/docs/)                                                                                                                                                                                        | Userspace kernel sandbox for stronger container isolation.               |
| [Kata Containers](https://github.com/kata-containers/kata-containers/blob/main/docs/design/architecture/README.md)                                                                                                        | Container UX backed by lightweight VMs.                                  |
| [Firecracker](https://github.com/firecracker-microvm/firecracker) / [Cloud Hypervisor](https://www.cloudhypervisor.org/)                                                                                                  | MicroVM backends for untrusted service and agent capsules.               |
| [WASI](https://github.com/WebAssembly/WASI) / [Wasmtime](https://docs.wasmtime.dev/)                                                                                                                                      | Portable capability-oriented system interface for WebAssembly workloads. |
| [Unikraft](https://unikraft.org/) / [MirageOS](https://mirage.io/)                                                                                                                                                        | Unikernel and library-OS targets for small service capsules.             |
| [Zephyr](https://www.zephyrproject.org/learn-about/) / [FreeRTOS](https://freertos.org/Why-FreeRTOS/What-is-FreeRTOS) / [Apache NuttX](https://nuttx.apache.org/docs/12.2.1/index.html) / [RTEMS](https://www.rtems.org/) | RTOS sidecar families for embedded and real-time control.                |
| [QNX Neutrino](https://blackberry.qnx.com/en/products/neutrino-rtos/neutrino-rtos)                                                                                                                                        | Commercial mission-critical RTOS adapter target when licensing allows.   |
| [Genode/Sculpt OS](https://genode.org/)                                                                                                                                                                                   | Component-based high-security OS research target.                        |
| [Qubes OS](https://doc.qubes-os.org/en/latest/developer/system/architecture.html)                                                                                                                                         | Security-by-compartmentalization architecture reference.                 |
| [Redox OS](https://www.redox-os.org/)                                                                                                                                                                                     | Rust microkernel research/watch target.                                  |
| [Fuchsia/Zircon](https://fuchsia.googlesource.com/fuchsia/+/b807c0ef08dc/docs/concepts/kernel/README.md)                                                                                                                  | Capability-oriented non-Linux kernel watch target.                       |
| [ReactOS](https://reactos.org/faq) / [Haiku](https://www.haiku-os.org/docs/welcome/welcome_en.html)                                                                                                                       | Compatibility OS watch targets, not near-term AIOS host kernels.         |

## 6. Closed kernel personality enum

```text
KernelPersonality =
  LINUX_GENERAL
| LINUX_HARDENED
| LINUX_PREEMPT_RT
| FREEBSD_SERVER
| FREEBSD_APPLIANCE
| ILLUMOS_SMARTOS
| OPENBSD_RESEARCH
| NETBSD_PORTABLE_RESEARCH
| DRAGONFLYBSD_RESEARCH
| RTOS_SIDECAR
| ZEPHYR_RTOS
| FREERTOS_RTOS
| NUTTX_RTOS
| RTEMS_RTOS
| QNX_NEUTRINO_RTOS
| MICROKERNEL_RESEARCH
| SEL4_MICROKERNEL_RESEARCH
| GENODE_SCULPT_RESEARCH
| REDOX_RESEARCH
| FUCHSIA_ZIRCON_WATCH
| GVISOR_USERSPACE_KERNEL
| MICROVM_RUNTIME
| UNIKERNEL_RUNTIME
| WASI_SYSTEM_INTERFACE
| QUBES_XEN_COMPARTMENTALIZATION
| ANDROID_CONTAINER
| WINDOWS_NT_VM_HOSTED
| VM_HOSTED_KERNEL
```

Unknown values are rejected by the kernel portability loader, capsule solver,
security profile validator, and recovery UI.

## 7. Support tier model

| Tier             | Meaning                                                            | Initial target                                                         |
| ---------------- | ------------------------------------------------------------------ | ---------------------------------------------------------------------- |
| `T0_PRIMARY`     | Full product path. CI, installer, UI, capsules, updates, evidence. | `LINUX_GENERAL`, `LINUX_HARDENED`                                      |
| `T1_SPECIALIZED` | Supported for a defined workload class, not full desktop parity.   | `LINUX_PREEMPT_RT`, `FREEBSD_SERVER`, `FREEBSD_APPLIANCE`              |
| `T2_HOSTED`      | Runs as VM, sidecar, or capsule backend under another AIOS host.   | `VM_HOSTED_KERNEL`, `RTOS_SIDECAR`                                     |
| `T3_RESEARCH`    | Architecture research; no user-facing promise.                     | `OPENBSD_RESEARCH`, `NETBSD_PORTABLE_RESEARCH`, `MICROKERNEL_RESEARCH` |
| `UNSUPPORTED`    | Detected but not admitted for AIOS-managed workloads.              | Any unknown or incomplete backend                                      |

## 8. Candidate target registry

S18 treats "kernel plugging" as several adapter families. Some are real host
kernels, some are system interfaces, and some are isolation substrates. The
registry records the value and admission level before implementation begins.

| Target                                              | Adapter family              | Initial tier                    | Why it matters                                                                             | Main limit                                                          |
| --------------------------------------------------- | --------------------------- | ------------------------------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------- |
| Linux general/hardened                              | Host kernel                 | `T0_PRIMARY`                    | Hardware, desktop, gaming, containers, GPU/video, package ecosystem.                       | Needs hardening profile discipline.                                 |
| Linux PREEMPT_RT                                    | Host kernel / RT            | `T1_SPECIALIZED`                | Best near-term answer for low-latency industrial/audio/control workloads.                  | Not a hard RTOS replacement for all safety cases.                   |
| FreeBSD                                             | Host kernel                 | `T1_SPECIALIZED`                | Jails, Capsicum, bhyve, ZFS, server/appliance security.                                    | Desktop/gaming/GPU parity is not the goal.                          |
| illumos/SmartOS                                     | Host/hypervisor appliance   | `T2_HOSTED` -> `T1_SPECIALIZED` | ZFS, zones, DTrace, storage and hypervisor appliance value.                                | Smaller hardware/app ecosystem.                                     |
| OpenBSD                                             | Host kernel research        | `T3_RESEARCH`                   | pledge/unveil and conservative security posture.                                           | Not a broad container/gaming target.                                |
| NetBSD rump                                         | Userspace kernel components | `T3_RESEARCH`                   | Portable kernel components in userspace; useful for filesystem/service isolation research. | Not a full AIOS desktop target.                                     |
| gVisor                                              | Userspace kernel sandbox    | `T1_SPECIALIZED`                | Stronger container isolation without full VM cost.                                         | Linux syscall compatibility is partial by design.                   |
| Kata / Firecracker / Cloud Hypervisor               | MicroVM runtime             | `T1_SPECIALIZED`                | Separate guest kernel for untrusted agents, services, and package conversion jobs.         | GPU and desktop integration are limited.                            |
| WASI / Wasmtime                                     | System interface runtime    | `T1_SPECIALIZED`                | Portable, capability-oriented apps and plugins across kernels.                             | Not a POSIX/Linux replacement for all apps.                         |
| Unikraft / MirageOS / Hermit                        | Unikernel runtime           | `T2_HOSTED`                     | Tiny service capsules, fast boot, small attack surface.                                    | Requires workload-specific builds and tooling.                      |
| Zephyr / FreeRTOS / NuttX                           | RTOS sidecar                | `T2_HOSTED`                     | Embedded controllers, sensors, hardware-adjacent real-time tasks.                          | Not a general OS; no normal desktop/process model.                  |
| RTEMS                                               | RTOS sidecar                | `T2_HOSTED`                     | POSIX-oriented real-time and aerospace/industrial heritage.                                | Single-address-space designs require strict trust boundaries.       |
| QNX Neutrino                                        | Commercial RTOS sidecar     | `T2_HOSTED`                     | Mission-critical automotive/medical/robotics cases.                                        | Licensing and redistribution constraints.                           |
| seL4                                                | Microkernel research        | `T3_RESEARCH`                   | Formal verification and high-assurance separation model.                                   | Needs a built system around the kernel.                             |
| Genode/Sculpt                                       | Component OS research       | `T3_RESEARCH`                   | Capability-based component architecture for secure desktops/appliances.                    | Not a near-term mainstream app ecosystem.                           |
| Qubes/Xen model                                     | Security architecture       | `T3_RESEARCH`                   | Compartmentalized desktop model and device-domain separation.                              | Hardware and UX cost; AIOS should reuse patterns, not become Qubes. |
| Android/Waydroid                                    | Containerized subsystem     | `T1_SPECIALIZED`                | Mobile app compatibility on Linux hosts.                                                   | Android security/app assumptions differ from Linux desktop.         |
| Windows NT hosted                                   | VM compatibility target     | `T2_HOSTED`                     | Legacy enterprise app fallback when Wine/Proton fails.                                     | Licensing and resource cost.                                        |
| ReactOS / Haiku / Redox / Fuchsia / MINIX / HelenOS | Watch targets               | `T3_RESEARCH` or `UNSUPPORTED`  | Useful ideas and compatibility experiments.                                                | Not near-term production backends.                                  |

Priority rule:

```text
implement first:
  Linux hardened
  Linux PREEMPT_RT
  FreeBSD server/appliance
  microVM runtime
  WASI runtime
  RTOS sidecar contract

research next:
  gVisor/Kata variants
  illumos/SmartOS
  OpenBSD/NetBSD rump
  seL4/Genode

watch only:
  ReactOS, Haiku, Redox, Fuchsia/Zircon, MINIX, HelenOS, DragonFlyBSD
```

## 9. Universal backend adapter mechanism

Kernel-dependent behavior is implemented through signed
`KernelBackendAdapter` packages. An adapter is not a normal app plugin. It is a
privileged system component and follows S16/S9 recovery rules.

```text
KernelBackendAdapter =
  adapter_id
  kernel_personality
  supported_kernel_versions
  support_tier
  capability_matrix_ref
  enforcement_bindings
  probe_commands
  evidence_emitters
  fallback_plan
  security_profile_compatibility
  signature_chain
```

Adapter lifecycle:

```text
discover host kernel
  -> load signed adapter manifest
  -> run passive probes
  -> build KernelCapabilityMatrix
  -> compare requested workload needs
  -> admit, degrade, route to VM, or block with reason
  -> emit evidence
```

Hard rules:

- Kernel adapters are allow-listed by AIOS release policy.
- Installing or replacing an adapter for `T0_PRIMARY` or `T1_SPECIALIZED`
  requires recovery approval.
- An adapter may expose only capabilities it can prove through probes.
- A failed probe must lower the capability matrix, not silently assume support.
- Kernel adapters cannot approve their own security exceptions.
- AI subjects may propose adapter changes but cannot apply them.

## 10. Kernel capability matrix

Every host produces one machine-readable `KernelCapabilityMatrix`:

```yaml
kernel_capability_matrix:
  host_id: "host_<ULID>"
  kernel_personality: LINUX_HARDENED
  kernel_release: "example"
  support_tier: T0_PRIMARY
  probes:
    timestamp_utc: "2026-05-28T00:00:00Z"
    evidence_receipt_id: "evr_..."
  primitives:
    sandbox:
      namespaces: present
      jails: absent
      pledge: absent
      unveil: absent
      capsicum: absent
      vm_boundary: present
    mac:
      selinux: enforcing
      apparmor: absent
      freebsd_mac: absent
      openbsd_base_hardening: absent
    resource_control:
      cgroups_v2: present
      rctl: absent
      cpu_sets: present
    realtime:
      preempt_rt: absent
      isolated_cpus: present
      irq_threading: present
    containers:
      oci_rootless: present
      freebsd_jail: absent
      linuxulator: absent
    virtualization:
      kvm: present
      bhyve: absent
      vmm: absent
    network:
      nftables: present
      pf: absent
      wireguard: present
    gpu_video:
      drm_kms: present
      vulkan: present
      vaapi: present
      nvenc: conditional
    filesystem:
      btrfs: present
      zfs: conditional
      snapshots: present
      verity: present
    audit:
      linux_audit: present
      bsm_audit: absent
```

The matrix is evidence-backed. Policy decisions must use the matrix, not
hard-coded assumptions like "Linux always has cgroups" or "BSD always has
jails."

## 11. Backend binding table

| AIOS need         | Linux backend                                     | FreeBSD backend                    | OpenBSD research backend       | Fallback             |
| ----------------- | ------------------------------------------------- | ---------------------------------- | ------------------------------ | -------------------- |
| Process isolation | namespaces, seccomp, Landlock                     | jails, Capsicum                    | pledge, unveil                 | VM capsule           |
| MAC policy        | SELinux/AppArmor                                  | MAC Framework                      | base hardening + pledge/unveil | deny or VM           |
| Resource control  | cgroups v2, cpuset                                | rctl, cpuset                       | limited                        | VM or block          |
| Containers        | OCI/Podman/containerd                             | jails, OCI where available         | not baseline                   | VM                   |
| Network policy    | nftables, bpfilter/eBPF where approved            | pf/ipfw                            | pf                             | brokered VM network  |
| GPU/video         | DRM/KMS, Mesa, NVIDIA, VAAPI/NVENC                | limited by hardware/driver path    | not baseline                   | Linux host/VM        |
| Real-time         | PREEMPT_RT, CPU isolation                         | limited/specialized                | not baseline                   | RTOS sidecar         |
| Storage/snapshots | btrfs, ZFS, dm-verity                             | ZFS, UFS snapshots where available | filesystem-specific            | block capsule        |
| Audit             | Linux audit, journald, eBPF probes where approved | BSM/auditd, syslog                 | syslog/accounting              | AIOS evidence broker |

## 12. Capsule solver integration

The S17 capsule solver must treat the kernel as an input constraint.

```text
capsule request
  + app/package/runtime metadata
  + security profile
  + KernelCapabilityMatrix
  -> backend selection
```

Selection outcomes:

| Outcome               | Meaning                                                                            |
| --------------------- | ---------------------------------------------------------------------------------- |
| `RUN_NATIVE`          | Host kernel has the required primitives.                                           |
| `RUN_DEGRADED`        | Host can run the capsule with declared missing features and operator-visible risk. |
| `RUN_HOSTED_VM`       | Workload is routed to a VM because native primitives are insufficient.             |
| `RUN_REMOTE`          | Workload is routed to another AIOS host with a better kernel personality.          |
| `BLOCKED_WITH_REASON` | No admitted backend can satisfy policy and workload requirements.                  |

Examples:

- Windows gaming prefers Linux + Proton. On FreeBSD, route to Linux VM or remote
  Linux gaming host unless a tested path exists.
- Hardened storage appliance may prefer FreeBSD + ZFS + jails.
- Real-time control prefers `LINUX_PREEMPT_RT` first, then `RTOS_SIDECAR` for
  stronger determinism.
- OpenBSD research backend may admit small service capsules with pledge/unveil
  mapping, not broad desktop/gaming workloads.

## 13. RTOS and dual-kernel rule

AIOS may support a dual-personality system where normal business workloads run
on Linux/FreeBSD and hard real-time work runs in `LINUX_PREEMPT_RT` or an
`RTOS_SIDECAR`.

The rule is strict:

```text
business/control plane != hard real-time authority
```

RTOS sidecars communicate through typed broker channels, signed command
schemas, monotonic sequence ids, timeout contracts, and evidence receipts. The
normal AIOS side must not directly poke RTOS memory, devices, or scheduler state
unless a safety-certified adapter explicitly grants that path.

### 13.1 Real-time workload contracts

The AIOS-side control plane drives an RT session through five closed contracts.
A session that cannot fill every required field of `RTWorkloadManifest` is
refused by `RTAdmissionController` before launch (fail-closed).

```text
RTWorkloadManifest =
  manifest_id                 # "rtw_<ULID>"
  workload_label
  deadline_us                 # required completion deadline per cycle, microseconds
  period_us                   # cycle period, microseconds
  jitter_budget_us            # max tolerated jitter, microseconds
  cpu_affinity                # explicit, exclusive CPU core set requested
  memory_lock                 # MLOCK_REQUIRED | MLOCK_NONE; locked pages, no paging
  device_needs                # list of { device_path, irq, access: EXCLUSIVE | SHARED }
  network_needs               # list of { interface_or_none, latency_class } or NONE
  target_personality          # LINUX_PREEMPT_RT | RTOS_SIDECAR
```

```text
RTAdmissionController =
  admission_id
  manifest_ref                # RTWorkloadManifest.manifest_id
  hardware_fit                # FIT | UNFIT  (against S8.3 hardware graph + KernelCapabilityMatrix.realtime)
  cpu_isolation_plan          # cores removed from general scheduler for this session
  irq_routing_plan            # IRQ affinity moved off / onto the isolated cores
  decision                    # ADMITTED | REFUSED
  refusal_reason              # null unless REFUSED (e.g. NO_PREEMPT_RT, INSUFFICIENT_ISOLATED_CPUS)
  policy_decision_id          # S2.3 decision; human approval required (no AI-started RT session)
  evidence_receipt_id
```

```text
RTLatencyEvidence =
  session_id
  manifest_ref
  observed_max_latency_us     # cyclictest-like worst-case observed latency
  missed_deadline_count
  irq_interference_events
  cpu_throttle_events
  window                      # measurement window (start_utc, end_utc) + TimeTrustGrade
  verdict                     # WITHIN_BUDGET | DEGRADED | VIOLATED
  evidence_receipt_id
```

```text
RTDeviceBinding =
  session_id
  bindings                    # list of { device_path, irq, access: EXCLUSIVE | SHARED }
  exclusivity_proof           # evidence that EXCLUSIVE bindings are not shared with non-RT subjects
  evidence_receipt_id

RTTeardownPlan =
  session_id
  cpu_restore                 # cores returned to the general scheduler
  irq_restore                 # IRQ affinity restored to pre-session state
  device_release              # device_paths/IRQs released back to normal AIOS
  state_verified              # PASSED | FAILED  (host returned to pre-RT baseline)
  evidence_receipt_id
```

Real-time invariants (enforced, not advisory):

- AI subjects cannot start an RT session without a human-approved
  `policy_decision_id` (INV-002).
- An RT session cannot disable evidence, the Policy Kernel, SELinux/MAC, or the
  recovery boundary.
- An RT workload receives only its declared devices and CPU cores
  (`RTDeviceBinding.exclusivity_proof` required for `EXCLUSIVE`).
- Missed deadlines emit `RTLatencyEvidence`; repeated `VIOLATED` verdicts degrade
  or stop the session.
- An RT profile cannot be promoted (§4.2) unless `RTLatencyEvidence.verdict` is
  `WITHIN_BUDGET` on the target hardware.

Unknown values for `memory_lock`, `target_personality`, `decision`, `verdict`,
`access`, or `state_verified` are rejected by `RTAdmissionController` and the
sidecar broker.

### 13.2 Sidecar broker wire contract

The normal AIOS side and an `RTOS_SIDECAR` (or `LINUX_PREEMPT_RT` RT domain)
communicate only through the broker. The broker is the sole channel; direct
memory/device/scheduler access is forbidden (see the strict rule above).

```text
RTBrokerChannel =
  channel_id
  direction                   # AIOS_TO_RT | RT_TO_AIOS
  channel_kind                # COMMAND | TELEMETRY | RECEIPT
  bound_session_id            # one RT session; channels are not shared across sessions
  signing_key_ref             # operator/trust-root key; AI-signed frames are rejected
```

```text
RTBrokerCommand =                # AIOS_TO_RT, COMMAND channel
  frame_id
  session_id
  sequence_id                  # strictly monotonic per channel; gaps/reordering -> rejected
  command_type                 # START | STOP | PARAM_UPDATE | TEARDOWN  (closed enum)
  payload                      # typed per command_type; no free-form shell
  issued_at_utc                # + TimeTrustGrade
  timeout_us                   # deadline for an acknowledging receipt; on miss -> session DEGRADED
  signature                    # over (session_id, sequence_id, command_type, payload, timeout_us)
```

```text
RTBrokerReceipt =                # RT_TO_AIOS, RECEIPT channel
  frame_id
  session_id
  acks_sequence_id             # the RTBrokerCommand.sequence_id being acknowledged
  result                       # ACCEPTED | REJECTED | TIMED_OUT | ERROR  (closed enum)
  reason                       # null unless REJECTED/ERROR
  observed_at_utc              # + TimeTrustGrade
  evidence_receipt_id          # emits RTOS_SIDECAR_COMMAND_RECEIPT
```

Timeout contract: every `RTBrokerCommand` carries a `timeout_us`; if no
`RTBrokerReceipt` with a matching `acks_sequence_id` arrives within that window,
the broker marks the command `TIMED_OUT`, downgrades the session, and emits
evidence. A `TIMED_OUT` or `REJECTED` `START`/`PARAM_UPDATE` never silently
proceeds.

Sequence rule: `sequence_id` is strictly monotonic per `RTBrokerChannel`; a
duplicate, gap, or out-of-order id is rejected, not reordered. Binding a sidecar
emits `RTOS_SIDECAR_BOUND`; each acknowledged command emits
`RTOS_SIDECAR_COMMAND_RECEIPT`.

Unknown values for `direction`, `channel_kind`, `command_type`, or `result` are
rejected by the broker. An unsigned or AI-signed frame is rejected (INV-028 for
boot/RT-domain integrity; INV-002 for propose-not-execute).

## 14. Security profile compatibility

S16 profiles constrain which kernel personalities may be admitted:

| Security profile | Admitted personalities                                                |
| ---------------- | --------------------------------------------------------------------- |
| `DEV_RELAXED`    | Any signed adapter, including `T3_RESEARCH`, with warning.            |
| `SECURE_DEFAULT` | `T0_PRIMARY`, `T1_SPECIALIZED`, `T2_HOSTED` with passing probes.      |
| `STIG_ALIGNED`   | `LINUX_HARDENED` first; FreeBSD only after control-map parity exists. |
| `AIRGAP_HIGH`    | Signed local adapters only; no research personalities.                |

BSD or microkernel support cannot weaken SELinux/STIG claims. If a non-Linux
personality lacks a control, the profile must record `NOT_APPLICABLE`,
`EQUIVALENT_CONTROL`, `EXCEPTION`, or `BLOCKED`.

## 15. Evidence records

S18 adds these evidence record types:

```text
KERNEL_PERSONALITY_DETECTED
KERNEL_BACKEND_ADAPTER_LOADED
KERNEL_CAPABILITY_MATRIX_BUILT
KERNEL_CAPABILITY_PROBE_RESULT
KERNEL_BACKEND_SELECTED_FOR_CAPSULE
KERNEL_BACKEND_DEGRADED
KERNEL_BACKEND_BLOCKED
KERNEL_BACKEND_FALLBACK_TO_VM
KERNEL_ADAPTATION_REQUESTED
KERNEL_CANDIDATE_BUILT
KERNEL_CANDIDATE_SIMULATED
KERNEL_CANDIDATE_CANARY_BOOTED
KERNEL_CANDIDATE_PROMOTED
KERNEL_CANDIDATE_ROLLED_BACK
RTOS_SIDECAR_BOUND
RTOS_SIDECAR_COMMAND_RECEIPT
```

Minimum fields for `KERNEL_CAPABILITY_PROBE_RESULT`:

```text
probe_id
kernel_personality
adapter_id
capability
status: PRESENT | ABSENT | CONDITIONAL | FAILED | UNKNOWN
observed_redacted
required_for_profiles
evidence_receipt_id
```

## 16. Non-goals

- Do not promise a single installed root filesystem can boot under both Linux
  and BSD with identical behavior.
- Do not build custom kernels as routine background optimization when no
  workload/profile benefit exists.
- Do not let AI promote, sign, or activate kernel candidates.
- Do not make BSD support block the Linux workstation/gaming path.
- Do not treat Linux compatibility layers as full Linux kernel equivalence.
- Do not expose kernel adapters as normal user-installable plugins.
- Do not allow a kernel backend to bypass the Policy Kernel, Sandbox
  Composition, Recovery Boundary, or Evidence Log.

## 17. Acceptance criteria

S18 is `REAL` only when:

1. `KernelPersonality` and `KernelCapabilityMatrix` are implemented as closed
   schemas.
2. At least one Linux adapter produces a real matrix from host probes.
3. S17 capsule solver consumes the matrix before selecting native, container,
   VM, or blocked execution.
4. Security profile validation rejects a kernel personality that cannot satisfy
   mandatory profile controls.
5. Adapter install/update is signed and recovery-gated.
6. Capability probe results emit evidence.
7. Unsupported BSD/RTOS/microkernel targets are reported as unsupported or
   research, not silently treated as Linux-compatible.
8. A workload that cannot run on the active kernel gets a clear fallback or
   `BLOCKED_WITH_REASON`.
9. Custom kernel builds require an explicit workload/profile trigger.
10. Kernel candidates pass isolated build, simulation, canary boot, evidence,
    and rollback gates before promotion.

## 18. See also

- [S16 Security Hardening and Compliance](../S16_Security_Hardening_Compliance/00_overview.md)
- [S17 App Capsule Runtime](../S17_App_Capsule_Runtime/00_overview.md)
- [S19 Driver and Firmware Capsule Plane](../S19_Driver_Firmware_Capsule_Plane/00_overview.md)
- [Rev.3 Planning Notes](../00_PLANNING_NOTES.md)
