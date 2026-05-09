# Hardware Graph (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists; structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Phase tag      | S8.3                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Layer          | L8 Network, Hardware, Devices                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Schema package | `aios.hardware.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Consumes       | **Imports vocabulary from**: L0 INV closed-id references (INV-002, INV-004, INV-005, INV-008, INV-012, INV-013, INV-014, INV-024); S0.1 typed action shape (cross-cutting, type-level); S1.3 AIOS-FS content-addressed object schema (type-level — HardwareGraph is stored as such); S2.3 Policy Kernel (`RemovableDevicePolicy` closed enum, hard-deny vocabulary — type-level); S3.1 Evidence Log (record append shape, FOREVER retention class — type-level); S3.2 Sandbox Composition (sandbox `device_policy` floor schema — type-level); S4.1 Namespace Layout (`/aios/system/hardware/...` paths closed enum — type-level); S5.1 Identity Model (`_system:service:hardware-manager` subject canonical-id, `recovery_mode` flag — type-level); S5.2 Vault Broker (HDM `KEY_SIGN` capability schema — type-level); S9.1 Recovery Boundary (recovery-mode default removable policy `DENY_DEFAULT` — type-level discipline reference). **Peer (intra-L8)**: S8.1 Network Policy (network-class devices feed network-policy enforcement), S8.2 GPU Resource Model (`GpuDevice` topology; GPU is one `DeviceClass` value; capability-lie discipline reused), S8.4 Firmware Trust (signed firmware blobs; version-monotonic check — peer L8 dependency). **Inverted direction (vocabulary owned here, consumed upward)**: ~~S11.1 Repository Model (firmware blobs as signed packages)~~ — this is the inversion: L10 must import vocabulary from L8.5 firmware-trust schema (S8.5 is the canonical owner of firmware-blob trust shape), not the other way around. Reframed as **L10/S11.1 imports vocabulary from S8.5**; this upward `Consumes` declaration is struck. **Lower-numbered runtime substrate**: S9.2 First-Boot Flow (initial graph snapshot at first boot — peer L1; consumed at first-boot HARDWARE_FIT prerequisites), S9.3 Dedicated Kernel Pipeline (consumes `hardware_graph_snapshot_id` as canonical input — peer L1; this is L9.3 _consuming_ L8's output, declared here for cross-reference clarity, not a runtime requirement on L9.3). |
| Produces       | typed `HardwareGraph` (content-addressed snapshot; id format `hwgraph_<hex32>`); closed `DeviceClass` (16); closed `DeviceTrustClass` (5); closed `DeviceLifecycleState` (8); closed `RemovableDevicePolicy` (5); closed `BusKind` (8); closed `DriverProvenance` (5); closed `DeviceQuarantineReason` (8); closed `HardwareGraphErrorCode`; `HardwareManagerService` gRPC; per-host hardware enumeration discipline at every boot; cross-boot drift detection against the prior `hardware_graph_snapshot_id`; signed driver binding registry; capability-advertised vs. capability-observed lie detection (`HOST_CAPABILITY_LIE` rationale, reused from S8.2 §8); 15 evidence record types queued for S3.1 (post-W9); per-device removable-device policy enforcement under operator approval; AI-blocked removable-device discipline (binds INV-013); driver-provenance taxonomy (preferred AIOS-verified > signed-kernel > distro-provided > out-of-tree-blacklisted; firmware-trusted is a separate axis from S8.4); IOMMU-DMA protection floor for Thunderbolt/USB4; closed-vocabulary classification of every detected device; the **HardwareGraph is the input artefact for S9.3 dedicated kernel builds**; the **HardwareGraph cross-boot diff is a constitutional drift signal** for evil-maid attacks; one queued L0 invariant candidate (`HARDWARE_GRAPH_DRIFT_FOREVER`); three queued S3.1 retention class assignments; five S2.3 condition fields queued (`target.device_class`, `target.device_trust_class`, `target.removable`, `target.driver_provenance`, `target.firmware_trusted`)                                                                                                                                                                                                                                                                                                                                                                                                                                                              |

## 1. Purpose

A live AIOS host carries a richly populated set of physical and virtual devices: CPUs, integrated and discrete GPUs, NVMe and SATA disks, Ethernet and Wi-Fi adapters, Bluetooth radios, audio inputs and outputs, USB hubs and the devices behind them, Thunderbolt root ports and DMA-capable docks behind them, temperature sensors, batteries, TPMs, security keys. Every one of these is, in classical Linux, a node in a sysfs tree that a privileged process may inspect or mutate. AIOS treats this surface differently. The **HardwareGraph** is the canonical, typed, content-addressed model of every hardware device on the host, produced at every boot, signed by the L8 Hardware Manager, and consumed by Policy Kernel, sandbox enforcement, GPU resource model, dedicated kernel pipeline, evidence log, and the operator's recovery flow.

Without this contract:

- INV-004 (recovery boundary) has no expression at the device layer — a USB stick plugged in during recovery has no statutory class to refuse against.
- INV-013 (AI cannot perform system admin) is silent on whether an AI subject may, for instance, mount a removable USB drive into `/aios`. The L8 device path needs a mechanical hard-deny.
- S8.2 §8 (`HOST_CAPABILITY_LIE`) is a discipline applied to GPU drivers; the same discipline must extend to every driver that reports an ABI version (storage, network, audio). The HardwareGraph is the registry where claimed and observed capabilities are pinned for cross-check.
- S9.2 first-boot has no canonical artefact recording **what hardware the host had at first install**, so a swapped Wi-Fi card or a replaced NVMe between boots is silently invisible.
- S9.3 dedicated kernel pipeline consumes `hardware_graph_snapshot_id` as `BuildRequest.hardware_graph_snapshot_id`. Without this spec, the **producer** of that id has no contract; the kernel pipeline is dangling at its input.
- USB-based attacks (BadUSB, juicing, mass-storage payloads) and Thunderbolt DMA attacks have no policy hook.
- An evil-maid hardware swap (replacing a network adapter with a malicious one, swapping a TPM module, slipping in a USB rubber-ducky during a coffee break) has no detection path.
- Firmware version downgrade attacks have no version-monotonic check at the device level.

This spec closes that loop. It is the **canonical hardware identification, classification, lifecycle, and policy plane** for AIOS. It is **not** the GPU resource model (S8.2 owns the GPU capability classes, VkDevice partitioning, dmabuf brokering — this spec defines `DeviceClass.GPU_INTEGRATED` / `GPU_DISCRETE` and how a GPU **enters** the graph; S8.2 owns what happens **after** it is recognised). It is **not** the network policy plane (S8.1 owns outbound/inbound policy — this spec catalogs `NETWORK_ETHERNET` / `NETWORK_WIFI` / `NETWORK_BLUETOOTH` adapters as graph nodes; S8.1 enforces traffic against them). It is **not** the firmware trust plane (S8.4 owns signature verification, vendor key pinning, version monotonicity rules at the firmware-blob level — this spec carries each device's `firmware_version` as a typed field and cites S8.4 for the verification rules).

This spec fixes:

1. The constitutional **HardwareGraph** as a content-addressed, signed, immutable per-boot snapshot.
2. The closed `DeviceClass` enum (16 values) — every detected device classifies into exactly one.
3. The closed `DeviceTrustClass` enum (5 values) — driver / firmware trust posture per device.
4. The closed `DeviceLifecycleState` enum (8 states) — `DETECTED → IDENTIFIED → DRIVER_BOUND → ACTIVE → DEGRADED → DISCONNECTED → QUARANTINED → RETIRED`.
5. The closed `RemovableDevicePolicy` enum (5 values) — `ALLOW_AUTO`, `REQUIRE_APPROVAL` (default), `DENY_DEFAULT`, `RECOVERY_ONLY`, `OPERATOR_AIRGAP`.
6. The closed `BusKind` (8) and `DriverProvenance` (5) auxiliary enums.
7. The detection pipeline: kernel udev events → Hardware Manager classification → driver binding evaluation → registration into the active graph.
8. The cross-boot drift detection mechanism using the prior `hardware_graph_snapshot_id` as anchor.
9. The capability-advertised vs. capability-observed lie discipline (extends S8.2 §8).
10. The removable-device approval flow with AI hard-deny.
11. Adversarial robustness: device-id spoofing, firmware downgrade, USB-borne attacks, Thunderbolt DMA, evil-maid hardware swap.
12. Fifteen evidence record types queued for S3.1 with retention classes (post-W9 — fourteen at Wave 8 close + `HARDWARE_GRAPH_DRIFT_ACCEPTED` from Wave 9 Cluster 8 SIM-C-002).
13. Three worked examples — boot-time graph build, USB device approval, evil-maid swap detection.

## 2. Core invariants

- **I1 — Closed device vocabulary.** `DeviceClass` is a closed enum with exactly 16 values plus the `_UNSPECIFIED` zero (§3.1). A device that cannot be classified into one of the 16 values is admitted under `OTHER` only; an `OTHER` device is `IDENTIFIED` but **not** `DRIVER_BOUND` until the next graph version recognises it. There is no per-host "custom" class; recognising a new device class is a versioned schema change.
- **I2 — Closed trust vocabulary.** `DeviceTrustClass` is closed at 5 values (§3.2). Every active device carries exactly one `DeviceTrustClass`; absence is treated as `OUT_OF_TREE_BLACKLISTED`.
- **I3 — Closed lifecycle.** `DeviceLifecycleState` is closed at 8 states (§3.3). Allowed transitions are listed in §6; back-transitions are forbidden except `DEGRADED → ACTIVE` (recovery from a transient health failure) and `QUARANTINED → RETIRED` (operator decision).
- **I4 — Content-addressed graph.** The active `HardwareGraph` is content-addressed: `hwgraph_<hex_lower(BLAKE3(JCS(graph)))[:32]>`. The graph is rebuilt at every boot; the resulting id is the canonical artefact emitted to evidence as `HARDWARE_GRAPH_REBUILT` and consumed by S9.3 kernel-build inputs as `BuildRequest.hardware_graph_snapshot_id`. The id space is global; collisions are not engineered against.
- **I5 — Signed by `_system:service:hardware-manager`.** Every `HardwareGraph` snapshot carries an Ed25519 signature from the HDM signing subject, whose private half is held in the Vault Broker (S5.2) under capability id `hardware.graph.sign`. Consumers (S9.3 kernel pipeline, S2.3 policy evaluation, S3.2 sandbox compilation) verify the signature before trusting the graph.
- **I6 — Cross-boot drift is constitutional.** On every boot, the new graph is diffed against the previous boot's `hardware_graph_snapshot_id` (read from `/aios/system/hardware/last_snapshot`). A non-empty diff that is not explained by a contemporaneous operator-approved hot-plug or quarantine emits `HARDWARE_GRAPH_DRIFT_DETECTED` evidence with `FOREVER` retention. This is the evil-maid swap signal (§9.5) and the L0 invariant candidate of this spec (`HARDWARE_GRAPH_DRIFT_FOREVER`).
- **I7 — INV-004 binds at the device layer.** Recovery-mode boots re-evaluate every device against `RemovableDevicePolicy.RECOVERY_ONLY` semantics: only constitutionally-required devices (CPU, primary disk, primary network, TPM, console-attached input/output, GPU primary head) advance past `IDENTIFIED`. Removable devices default to `DENY_DEFAULT` in recovery and require explicit operator approval to bind. This binds INV-004's mechanical recovery boundary to the device admission plane.
- **I8 — INV-013 binds at the device layer.** AI subjects (`subject.is_ai = true`) **cannot** issue any HDM mutating RPC (`ApproveRemovableDevice`, `QuarantineDevice`, `RebindDriver`, `AcceptDrift`). All such attempts emit `AI_REMOVABLE_DEVICE_BLOCKED` evidence with `FOREVER` retention. This is the device-plane expression of INV-013 (no AI system admin) and is enforced by the S2.3 hard-deny `AISystemAdminBlocked` for any action whose target resolves into `/aios/system/hardware/...`.
- **I9 — INV-024 binds at the device layer.** A `DeviceClass.GPU_INTEGRATED` or `GPU_DISCRETE` device that advertises compute capability does not, by appearing in the graph, grant any subject access to it. Every compute submission flows through the S8.2 capability-binding flow gated by INV-024's `gpu.compute_heavy` capability. The HardwareGraph **populates** S8.2's `GpuDevice.api_support`; the graph itself is **not** an authorisation channel.
- **I10 — Drivers are typed and signed.** Every `DeviceLifecycleState.DRIVER_BOUND` transition records a `driver_id` whose binary is signed (`SIGNED_KERNEL_DRIVER` provenance) or AIOS-verified (`AIOS_VERIFIED_DRIVER` provenance). `OUT_OF_TREE_BLACKLISTED` drivers refuse to bind by default; admitting one requires recovery-mode operator co-signer and emits `OUT_OF_TREE_DRIVER_BLOCKED` (FOREVER) on every refused attempt.
- **I11 — Capability lies are detected.** A driver that **advertises** support for an ABI version (storage protocol level, network adapter feature flag, audio sample-rate ceiling, GPU API version) but whose **observed** behaviour does not match the advertisement (probe returns mismatched value, kernel-level capability differs from user-space report) is recorded with both fields populated and emits `HOST_CAPABILITY_LIE` evidence with `FOREVER` retention. This generalises S8.2 §8 from GPU-specific to all device classes.
- **I12 — Removable-device default is `REQUIRE_APPROVAL`.** Out of the box, a freshly inserted USB mass-storage device, smartcard reader, or unknown HID device is held at `IDENTIFIED` pending operator approval. The default is **not** `ALLOW_AUTO`. The default in recovery mode is **`DENY_DEFAULT`**. The operator may, per group, raise the default to `ALLOW_AUTO` for specific `DeviceClass` values via a signed policy bundle (S2.3 §6.X — touch-up queued); this is a per-host posture choice, not a constitutional default.
- **I13 — Firmware version is monotonic.** A device whose firmware version, on any boot, is **less than** the version recorded in the previous `HardwareGraph` snapshot for the same device-identity tuple (`vendor_id`, `device_id`, `serial_number`) is held at `QUARANTINED` pending operator approval. The downgrade emits `FIRMWARE_VERSION_DOWNGRADE_BLOCKED` evidence with `FOREVER` retention. This binds S8.4 firmware-trust's version-monotonic discipline to the device admission flow.
- **I14 — IOMMU-DMA protection floor.** Devices on a DMA-capable bus (`BusKind.PCIE`, `BusKind.THUNDERBOLT`, `BusKind.USB4`) MUST be admitted only with IOMMU groups enforced. If `/sys/kernel/iommu_groups/` is empty or the device's IOMMU group is missing, the device is held at `IDENTIFIED` and the host emits `IOMMU_DMA_PROTECTION_DEGRADED` evidence (`EXTENDED_60M`). For Thunderbolt/USB4 devices specifically, admission outside IOMMU is **forbidden**; the device is parked at `QUARANTINED` until either the IOMMU is enabled (BIOS/firmware) or the operator co-signs a recovery-mode override.
- **I15 — Failure is closed.** Every failure path (`DeviceUnclassifiable`, `DriverBindRejected`, `RemovableDeviceDenied`, `FirmwareDowngradeDetected`, `IommuRequiredAbsent`, `DriftWithoutApproval`, `HostCapabilityLie`) results in the device staying out of `ACTIVE`. There is no "best effort" admission. A degradation (e.g., binding a DISTRO_PROVIDED_DRIVER instead of an AIOS_VERIFIED_DRIVER) is explicit and emits `DEVICE_DRIVER_BOUND` with the lower trust class recorded.
- **I16 — Append-only graph history.** Snapshots are never rewritten. Each boot's graph is a fresh content-addressed object; the previous `hardware_graph_snapshot_id` is retained for diff comparison and forensic chain-of-custody. Compaction follows S3.1's append-only retention discipline.

## 3. Vocabulary (closed enums)

All vocabularies in this section are closed. Adding a value is a versioned schema change. Bundle load fails on unknown values. None of these enums admits an open / "custom" / "user-defined" alternative.

### 3.1 `DeviceClass`

The closed list of hardware classifications recognised by the HardwareGraph. Every detected device is classified into exactly one value. The `_UNSPECIFIED` zero is reserved for proto compatibility and is not a valid runtime classification; a device with `class = DEVICE_CLASS_UNSPECIFIED` is rejected at registration.

```proto
enum DeviceClass {
  DEVICE_CLASS_UNSPECIFIED      = 0;
  CPU                           = 1;   // host CPU package(s)
  GPU_INTEGRATED                = 2;   // iGPU on CPU package; bridges to S8.2
  GPU_DISCRETE                  = 3;   // PCIe discrete GPU; bridges to S8.2
  DISK_NVME                     = 4;   // NVMe SSD
  DISK_SATA                     = 5;   // SATA HDD/SSD
  NETWORK_ETHERNET              = 6;   // wired NIC (1G / 2.5G / 10G)
  NETWORK_WIFI                  = 7;   // Wi-Fi adapter
  NETWORK_BLUETOOTH             = 8;   // Bluetooth radio
  AUDIO_OUTPUT                  = 9;   // DAC / speaker / headphone jack
  AUDIO_INPUT                   = 10;  // microphone / line-in
  USB_HUB                       = 11;  // USB hub (root or downstream)
  USB_DEVICE                    = 12;  // generic USB device behind a hub
  THUNDERBOLT_HOST              = 13;  // Thunderbolt / USB4 root
  SENSOR_TEMPERATURE            = 14;  // CPU / SSD / chassis thermal sensor
  SENSOR_BATTERY                = 15;  // battery / UPS sensor
  TPM_2_0                       = 16;  // TPM 2.0 module
  SECURITY_KEY                  = 17;  // FIDO2 / PIV hardware token
  OTHER                         = 18;  // typed escape valve; admitted IDENTIFIED-only
}
```

The 16-class taxonomy plus `OTHER` is deliberate: this set covers the realistic device population of a personal/homelab AIOS host. Cameras, printers, audio interfaces with both inputs and outputs, smartcard readers, and barcode scanners all fall under `USB_DEVICE` (with `model_string` giving the operator-readable name). Devices that need a finer class than the catalog allows go through a versioned schema bump, never through host-local extension.

### 3.2 `DeviceTrustClass`

The closed list of trust postures applied to a device's driver and firmware combination. Exactly one value per active device.

```proto
enum DeviceTrustClass {
  DEVICE_TRUST_CLASS_UNSPECIFIED   = 0;
  AIOS_VERIFIED_DRIVER             = 1;   // driver in the AIOS verified driver registry; preferred
  SIGNED_KERNEL_DRIVER             = 2;   // upstream-signed kernel driver; second preference
  DISTRO_PROVIDED_DRIVER           = 3;   // distro-supplied driver; third preference
  OUT_OF_TREE_BLACKLISTED          = 4;   // unsigned / out-of-tree; default deny
  FIRMWARE_TRUSTED                 = 5;   // device whose trust depends on firmware (e.g., NIC firmware path); cited per S8.4
}
```

`AIOS_VERIFIED_DRIVER` is the AIOS curated set: drivers that have been audit-reviewed and are co-signed by the AIOS root and the relevant vendor. `SIGNED_KERNEL_DRIVER` is upstream-signed (the same signing posture as Linux kernel modules with `MODULE_SIG`). `DISTRO_PROVIDED_DRIVER` is what most Linux installations ship today: from a distro repo, often unsigned at the AIOS layer but signed at the distro's package level. `OUT_OF_TREE_BLACKLISTED` is the default-deny floor: anything that does not satisfy the higher classes drops here. `FIRMWARE_TRUSTED` is the bridge to S8.4: certain devices' trust posture depends on firmware blobs (NIC microcode, GPU vbios, SSD controller firmware), and the trust verdict here is "the driver is acceptable conditional on the device's firmware passing S8.4's verification".

### 3.3 `DeviceLifecycleState`

The closed FSM state per device. State transitions are evidence-emitting (§7).

```proto
enum DeviceLifecycleState {
  DEVICE_LIFECYCLE_STATE_UNSPECIFIED = 0;
  DETECTED                           = 1;   // udev event observed; not yet classified
  IDENTIFIED                         = 2;   // classified into a DeviceClass; awaiting driver binding
  DRIVER_BOUND                       = 3;   // a driver has bound; not yet ACTIVE
  ACTIVE                             = 4;   // device is in use by AIOS subjects
  DEGRADED                           = 5;   // health probe failed; device may still serve at reduced reliability
  DISCONNECTED                       = 6;   // device removed (hot-unplug or hard-disconnect)
  QUARANTINED                        = 7;   // device held out of service; operator action required to clear
  RETIRED                            = 8;   // device permanently retired; will not be re-bound until manually re-introduced
}
```

The lifecycle table per `DeviceClass` is in §6.

### 3.4 `RemovableDevicePolicy`

The closed list of policy postures for removable devices. The default at host install is `REQUIRE_APPROVAL`; the recovery-mode default is `DENY_DEFAULT`.

```proto
enum RemovableDevicePolicy {
  REMOVABLE_DEVICE_POLICY_UNSPECIFIED = 0;
  ALLOW_AUTO                          = 1;   // automatic admission to ACTIVE; rare; per-class operator opt-in
  REQUIRE_APPROVAL                    = 2;   // default; HUMAN_USER must approve before binding
  DENY_DEFAULT                        = 3;   // default in recovery; admission requires operator co-signer
  RECOVERY_ONLY                       = 4;   // device may be admitted only during recovery mode
  OPERATOR_AIRGAP                     = 5;   // device-class is permanently denied except behind physical operator action (e.g., key swap)
}
```

`ALLOW_AUTO` is the looseest setting and is **never** the default. An operator may opt into `ALLOW_AUTO` for, say, `AUDIO_OUTPUT` devices on a workstation where USB headphones are a frequent reconnection event; even then, the choice is recorded, and AI subjects cannot mount the device (per I8). `REQUIRE_APPROVAL` is the constitutional default — every plugged-in USB stick requires the operator to approve before its filesystem becomes readable. `DENY_DEFAULT` is the recovery-mode default; recovery flows are not an opportunity for casual hot-plug attachment. `RECOVERY_ONLY` is a class for devices like recovery-only smartcards: they may **only** appear during a recovery flow and are denied outside of it. `OPERATOR_AIRGAP` is the strictest: the device class is never admitted by any policy bundle; lifting it requires a hardware-key co-signer in recovery mode plus FOREVER evidence.

### 3.5 `BusKind`

The closed list of bus topologies. Used for IOMMU enforcement decisions (§9.4) and to constrain certain `DeviceClass` values.

```proto
enum BusKind {
  BUS_KIND_UNSPECIFIED = 0;
  PCIE                 = 1;   // PCI Express
  USB2                 = 2;   // legacy USB 2.x
  USB3                 = 3;   // USB 3.x (xHCI)
  USB4                 = 4;   // USB4 / Thunderbolt-compatible
  THUNDERBOLT          = 5;   // legacy Thunderbolt
  SATA                 = 6;
  NVME_BUS             = 7;   // NVMe over PCIe; tracked separately for accounting
  PLATFORM             = 8;   // platform-bus / non-removable (sensors, TPM-on-LPC)
}
```

`PCIE`, `THUNDERBOLT`, `USB4` are DMA-capable; admitting devices on these buses requires IOMMU enforcement per I14.

### 3.6 `DriverProvenance`

The closed list of driver provenance values. Cross-references `DeviceTrustClass` but is finer-grained for forensic purposes.

```proto
enum DriverProvenance {
  DRIVER_PROVENANCE_UNSPECIFIED = 0;
  AIOS_REGISTRY                 = 1;   // driver came from the AIOS verified-driver registry
  KERNEL_MAINLINE               = 2;   // upstream Linux mainline; signed kernel module
  DISTRO_PACKAGE                = 3;   // distro-package supplied
  FIRMWARE_BUNDLE               = 4;   // firmware-only path (no kernel driver); cited per S8.4 / S11.1
  OUT_OF_TREE_REJECTED          = 5;   // out-of-tree; rejected at bind
}
```

### 3.7 `DeviceQuarantineReason`

The closed list of reasons a device may enter `QUARANTINED`.

```proto
enum DeviceQuarantineReason {
  DEVICE_QUARANTINE_REASON_UNSPECIFIED = 0;
  CAPABILITY_LIE                       = 1;   // advertised vs observed mismatch (I11)
  FIRMWARE_DOWNGRADE                   = 2;   // I13
  DRIFT_UNAPPROVED                     = 3;   // I6 cross-boot drift not approved
  IOMMU_REQUIRED_ABSENT                = 4;   // I14 on Thunderbolt/USB4
  REMOVABLE_DENIED                     = 5;   // I12 default-deny on removable
  HEALTH_REPEATED_FAILURE              = 6;   // health probe repeatedly failed
  POLICY_BUNDLE_DENY                   = 7;   // active policy explicitly denies
  OPERATOR_INITIATED                   = 8;   // operator manually quarantined
}
```

### 3.8 `HardwareGraphErrorCode`

The closed list of HDM RPC error codes.

```proto
enum HardwareGraphErrorCode {
  HARDWARE_GRAPH_ERROR_CODE_UNSPECIFIED = 0;
  GRAPH_NOT_INITIALIZED                 = 1;
  GRAPH_SIGNATURE_INVALID               = 2;
  DEVICE_UNCLASSIFIABLE                 = 3;
  DRIVER_BIND_REJECTED                  = 4;
  REMOVABLE_DEVICE_DENIED               = 5;
  FIRMWARE_DOWNGRADE_DETECTED           = 6;
  IOMMU_REQUIRED_ABSENT                 = 7;
  DRIFT_WITHOUT_APPROVAL                = 8;
  HOST_CAPABILITY_LIE                   = 9;
  AI_FORBIDDEN_OPERATION                = 10;  // INV-013 hard-deny
  RECOVERY_REQUIRED                     = 11;  // INV-012
  POLICY_REFUSED                        = 12;
  DEVICE_NOT_FOUND                      = 13;
}
```

## 4. The HardwareGraph object

### 4.1 Shape

```proto
message HardwareGraph {
  // hwgraph_<hex_lower(BLAKE3(JCS(self_minus_signature)))[:32]>
  string snapshot_id              = 1;
  google.protobuf.Timestamp built_at = 2;
  string previous_snapshot_id     = 3;        // empty on first boot
  repeated Device devices         = 4;
  HardwareGraphMeta meta          = 5;
  bytes ed25519_signature         = 6;        // by _system:service:hardware-manager
}

message HardwareGraphMeta {
  string host_id                  = 1;        // L4 host identity
  bool recovery_mode              = 2;        // boot mode at graph build
  bool iommu_enforced_globally    = 3;
  string boot_evidence_anchor     = 4;        // S9.2 first-boot marker hash; pins graph to host
  uint32 device_count             = 5;
}

message Device {
  string device_id                = 1;        // dev_<ulid>; stable across reboots iff identity tuple stable
  DeviceClass class               = 2;
  string model_string             = 3;        // operator-readable; e.g., "Intel Wi-Fi 6E AX211"
  string vendor_id                = 4;        // e.g., "0x8086"
  string device_id_pci_usb        = 5;        // e.g., "0x51f0"
  string serial_number            = 6;        // empty if not present
  BusKind bus_kind                = 7;
  string bus_path                 = 8;        // e.g., "0000:00:14.0" (PCI BDF) or "1-1.2" (USB)
  string driver_id                = 9;        // signed reference to an entry in /aios/system/drivers/<id>
  DriverProvenance driver_provenance = 10;
  string driver_version           = 11;
  string firmware_version         = 12;       // empty if device has no firmware path
  bool firmware_trusted           = 13;       // true iff S8.4 verified the firmware blob
  DeviceTrustClass trust_class    = 14;
  DeviceLifecycleState lifecycle_state = 15;
  RemovableDevicePolicy removable_policy = 16; // REMOVABLE_DEVICE_POLICY_UNSPECIFIED on non-removable
  bool removable                  = 17;
  bool iommu_group_present        = 18;
  uint32 iommu_group_id           = 19;       // 0 if not applicable
  CapabilityProbe capabilities_advertised = 20;
  CapabilityProbe capabilities_observed   = 21;
  google.protobuf.Timestamp first_seen_at = 22;
  google.protobuf.Timestamp last_seen_at  = 23;
}

message CapabilityProbe {
  // Closed bag of capability fields; exact contents per DeviceClass.
  // Examples:
  //   GPU: vulkan_version, opengl_version, ray_tracing_supported (delegates to S8.2)
  //   DISK_NVME: pcie_gen, queue_depth_max, namespace_count, encryption_in_place
  //   NETWORK_ETHERNET: link_speed_mbps_max, sr_iov_supported, mac_address (redacted-hash)
  //   NETWORK_WIFI: standards (802.11ac/ax/be), bands_supported, mac_address (redacted-hash)
  //   AUDIO_OUTPUT: sample_rate_max_hz, channel_count_max
  //   TPM_2_0: family ("2.0"), pcr_bank_count, ek_certificate_present
  //   SECURITY_KEY: fido2_supported, piv_supported
  // Concrete schema per class is in Appendix A.
  map<string, string> fields      = 1;
}
```

### 4.2 Content addressing and stability

The `snapshot_id` is computed as `hwgraph_<hex_lower(BLAKE3(JCS(graph)))[:32]>` where `JCS(graph)` is the JSON Canonicalisation Scheme (RFC 8785) over the `HardwareGraph` minus the `ed25519_signature` field. The signature is computed over the same canonical bytes. This matches the content-addressing pattern used elsewhere in AIOS (S1.3 object versions, S9.3 `BuildRequest.aios_invariant_bundle_id`, S9.2 first-boot marker hash) and lets S9.3 cite the snapshot id as a stable input to the kernel build pipeline.

`device_id` (per-device) is a ULID stable across boots **iff** the identity tuple `(vendor_id, device_id_pci_usb, serial_number, bus_path)` is stable. A swapped Wi-Fi card whose vendor/device id matches but whose serial differs gets a fresh `device_id`; this is the mechanical signal for I6 drift detection. Bus-path changes alone (e.g., a USB device replugged into a different port) do **not** mint a new `device_id`; the identity tuple's `serial_number` (when present) is the stronger anchor.

### 4.3 Persistence

The active `HardwareGraph` snapshot is written to `/aios/system/hardware/active` after every boot. The previous snapshot is retained at `/aios/system/hardware/previous`. Older snapshots are retained per S3.1 retention class for the corresponding evidence record (`HARDWARE_GRAPH_REBUILT` is `STANDARD_24M`; the evidence record carries the snapshot id, not the full graph; the graph itself is an AIOS-FS object whose retention is governed by the operator's storage budget).

### 4.4 First-boot bootstrapping

S9.2 §3.2 first-boot does not require a fully populated hardware graph (the generic kernel covers all supported hardware). However, S9.2's `STAGE_RUNTIME_SERVICES_STARTED` triggers the first hardware-graph build; the resulting `hwgraph_<...>` is recorded in the first-boot marker hash composition (S9.2 §3.2 marker definition) so that the graph is anchored to the first-boot constitutional commit. On every subsequent boot, the graph build is the prerequisite for the dedicated kernel pipeline (S9.3) to produce a valid `BuildRequest`.

## 5. Detection pipeline

### 5.1 Pipeline stages

```text
udev kernel event
   ─▶ HDM event listener (subscribes to udev netlink)
   ─▶ Classification (§5.2)
   ─▶ Driver binding evaluation (§5.3)
   ─▶ HardwareManager registration (§5.4)
   ─▶ Lifecycle FSM (§6)
```

The HDM service `_system:service:hardware-manager` is started early in S9.2 `STAGE_RUNTIME_SERVICES_STARTED` and runs continuously thereafter. On boot, it drains the kernel's accumulated udev events for already-present devices ("coldplug"), then transitions to streaming mode for hot-plug events.

### 5.2 Classification

Each udev event carries kernel-supplied identity attributes (`ID_VENDOR_ID`, `ID_MODEL_ID`, `SUBSYSTEM`, `DEVTYPE`, `ID_BUS`, etc.). The HDM:

1. Reads the kernel attributes.
2. Cross-checks them against the AIOS hardware database (a versioned, signed catalogue at `/aios/system/hardware/db`).
3. Assigns a `DeviceClass` value.
4. If no match exists, assigns `DeviceClass.OTHER` and freezes the device at `IDENTIFIED`.

A device-id spoofing attempt (where a malicious USB device claims a vendor/device id that does not match its actual behaviour) is caught here: the AIOS database has, for many vendor/device id pairs, a small fingerprint (e.g., SCSI inquiry response signature, USB descriptor full-content hash) that is matched against the freshly probed device. A mismatch produces a `HOST_CAPABILITY_LIE` signal (§9.1) and the device is parked at `QUARANTINED`.

### 5.3 Driver binding evaluation

For a classified device, the HDM walks the driver candidate list in `DeviceTrustClass` preference order:

1. `AIOS_VERIFIED_DRIVER` — preferred. If present and signature verifies, bind.
2. `SIGNED_KERNEL_DRIVER` — second preference.
3. `DISTRO_PROVIDED_DRIVER` — third preference.
4. `FIRMWARE_TRUSTED` (firmware-only paths, no kernel driver) — for devices that do not need a separate kernel driver.
5. `OUT_OF_TREE_BLACKLISTED` — refused unless an active policy bundle explicitly admits it (operator-co-signed, recovery-mode authored, FOREVER-evidenced). The default refusal emits `OUT_OF_TREE_DRIVER_BLOCKED` (FOREVER).

A successful bind transitions `IDENTIFIED → DRIVER_BOUND` and emits `DEVICE_DRIVER_BOUND` (`STANDARD_24M`). A refused bind transitions to `QUARANTINED` with reason `POLICY_BUNDLE_DENY` and emits `DEVICE_DRIVER_REJECTED` (`EXTENDED_60M`).

### 5.4 Registration

A `DRIVER_BOUND` device transitions to `ACTIVE` after passing a class-specific health probe (link state for network, device-ready for storage, presence-attest for TPM). The transition emits `DEVICE_DETECTED` (`STANDARD_24M`) at the first time the host sees this device-identity tuple, plus `DEVICE_DRIVER_BOUND` for the bind itself. Once the active `HardwareGraph` is rebuilt with the device entry present and `lifecycle_state = ACTIVE`, the snapshot id rotation emits `HARDWARE_GRAPH_REBUILT` (`STANDARD_24M`).

### 5.5 Hot-plug

Hot-plug events follow the same pipeline. The HardwareGraph is rebuilt on every hot-plug event that changes the active set (a non-policy-relevant event like a transient USB hub reset is debounced over a 250 ms window). Each rebuild produces a new `snapshot_id`; the previous id is retained for diff. If the new device is removable and its `RemovableDevicePolicy` is `REQUIRE_APPROVAL`, the device is held at `IDENTIFIED` and a `REMOVABLE_DEVICE_REQUEST` evidence record is emitted (`STANDARD_24M`); the operator approves via the typed `ApproveRemovableDevice` RPC (§8) before driver binding evaluation proceeds.

## 6. Lifecycle FSM

### 6.1 Allowed transitions

```text
DETECTED ─▶ IDENTIFIED                  (§5.2 classification)
IDENTIFIED ─▶ DRIVER_BOUND              (§5.3 driver bind)
IDENTIFIED ─▶ QUARANTINED               (capability lie, IOMMU absent on Thunderbolt, OUT_OF_TREE refusal, removable denied)
DRIVER_BOUND ─▶ ACTIVE                  (health probe pass)
DRIVER_BOUND ─▶ QUARANTINED             (firmware downgrade, drift unapproved)
ACTIVE ─▶ DEGRADED                      (health probe fail, transient)
ACTIVE ─▶ DISCONNECTED                  (hot-unplug)
ACTIVE ─▶ QUARANTINED                   (operator action, policy change)
DEGRADED ─▶ ACTIVE                      (health probe pass after recovery)
DEGRADED ─▶ DISCONNECTED                (hot-unplug)
DEGRADED ─▶ QUARANTINED                 (repeated failure)
QUARANTINED ─▶ DRIVER_BOUND             (operator approval; resume from quarantine)
QUARANTINED ─▶ RETIRED                  (operator decision; permanent)
DISCONNECTED ─▶ DETECTED                (re-plug; pipeline re-runs)
```

Forbidden transitions:

- Anything backward from `ACTIVE` to `IDENTIFIED` or `DETECTED`.
- `RETIRED` is terminal — manually re-introducing a retired device is not a transition; it is a fresh registration as a new device.
- `DISCONNECTED → ACTIVE` directly is forbidden; a re-plug runs the pipeline again from `DETECTED`.

### 6.2 Per-class lifecycle quirks

- `CPU` enters `ACTIVE` at boot and never transitions to `DEGRADED` per HDM (CPU thermal degradation is a sensor concern, recorded against `SENSOR_TEMPERATURE`); a CPU `DISCONNECTED` is implausible on a personal/homelab host and triggers `HARDWARE_GRAPH_DRIFT_DETECTED` (FOREVER).
- `TPM_2_0` and `SECURITY_KEY` are constitutionally significant: their `DISCONNECTED` transition under non-recovery conditions emits `HARDWARE_GRAPH_DRIFT_DETECTED` (FOREVER) and forces the host into a degraded session posture (sessions requiring TPM-quoted secrets are paused).
- `THUNDERBOLT_HOST` and devices behind it require IOMMU (I14); if absent at any boot, the entire Thunderbolt subtree is parked at `QUARANTINED`.

## 7. Cross-boot drift detection

### 7.1 The diff procedure

On boot, after the new graph is built, the HDM reads `previous_snapshot_id` (from `/aios/system/hardware/last_snapshot`), loads the corresponding graph from `/aios/system/hardware/previous`, and computes the symmetric difference of device entries keyed by identity tuple `(vendor_id, device_id_pci_usb, serial_number)`. The diff yields three sets:

- **Added**: identity tuples present in new, absent in previous.
- **Removed**: present in previous, absent in new.
- **Mutated**: present in both, but with changed `firmware_version`, `driver_id`, `bus_path`, or `capabilities_observed`.

### 7.2 Approved versus unapproved

A diff entry is **approved** if:

- The added device was inserted under an active `REMOVABLE_DEVICE_APPROVED` evidence record within the last 24 hours, or
- The removed device's removal was preceded by a `DEVICE_DISCONNECTED` evidence record within the previous boot session, or
- The mutation is an `OPERATOR_INITIATED` driver rebind or firmware update with a contemporaneous evidence record.

Any diff entry that is **not** approved emits `HARDWARE_GRAPH_DRIFT_DETECTED` evidence (`FOREVER`) carrying:

```text
{
  prior_snapshot_id:   "hwgraph_<hex32_prev>",
  current_snapshot_id: "hwgraph_<hex32_now>",
  added: [...],
  removed: [...],
  mutated: [...],
  operator_alert_raised: true
}
```

The operator-alert is mandatory for unapproved drift. The host does **not** auto-pause; the operator decides whether to enter recovery mode, accept the drift, or quarantine the affected devices.

### 7.3 Why FOREVER

Hardware drift is either a benign event (hardware repair, intentional upgrade) that the operator should have approved through the typed flow, or a malicious event (evil-maid swap, supply-chain interception). In either case the forensic record is required indefinitely. The volume is tiny — drift on a personal/homelab host is rare — so FOREVER retention is operationally affordable and forensically essential.

## 8. Removable-device approval flow

### 8.1 Default posture

Per I12, the default `RemovableDevicePolicy` is `REQUIRE_APPROVAL` outside recovery, `DENY_DEFAULT` inside recovery. Operator opt-in to `ALLOW_AUTO` is per-`DeviceClass` and is recorded in the active policy bundle (S2.3 §6.X — touch-up queued).

### 8.2 The approval RPC

```proto
rpc ApproveRemovableDevice(ApproveRemovableDeviceRequest)
    returns (ApproveRemovableDeviceResponse);
```

The request carries the `device_id`, the requesting `subject_canonical_id`, an optional `ttl` (default 24 h; never permanent for first approval), and an explicit policy class to apply (`ALLOW_AUTO_THIS_BOOT`, `ALLOW_AUTO_FOREVER`, `ALLOW_AUTO_FOR_GROUP_<g>`). The HDM validates:

1. The requesting subject is a `HUMAN_USER` (per S5.1). AI subjects are hard-denied with `AI_FORBIDDEN_OPERATION` and `AI_REMOVABLE_DEVICE_BLOCKED` evidence (`FOREVER`); this is the device-plane expression of INV-013.
2. The host is not in recovery mode — or if it is, the policy class is `ALLOW_AUTO_THIS_BOOT` only (recovery cannot grant persistent removable approvals).
3. The device's IOMMU posture is acceptable (I14).
4. The S2.3 policy bundle does not deny.

A successful approval transitions the device through driver-binding evaluation (§5.3), emits `REMOVABLE_DEVICE_APPROVED` (`STANDARD_24M`), and writes the typed grant to `/aios/system/hardware/grants/<grant_id>`. A refusal emits `REMOVABLE_DEVICE_DENIED` (`EXTENDED_60M`) carrying the refusal reason.

### 8.3 AI subject hard-deny

This is the one mechanical point where INV-013 binds to L8 hardware. Any AI-subject HDM mutating call (`ApproveRemovableDevice`, `QuarantineDevice`, `RebindDriver`, `AcceptDrift`) is rejected at the RPC envelope before reaching the HDM logic, with `AI_FORBIDDEN_OPERATION`. The S2.3 hard-deny `AISystemAdminBlocked` fires because the action target resolves to `/aios/system/hardware/...`. The evidence emitted is `AI_REMOVABLE_DEVICE_BLOCKED` (`FOREVER`) carrying the AI subject id, the attempted device id, the attempted RPC name, and the action envelope id (S0.1 reference).

### 8.4 Timeout and revocation

A removable device approved with `ALLOW_AUTO_THIS_BOOT` transitions to `DISCONNECTED` at host shutdown and may not auto-rebind on next boot. An approval with a TTL is revoked at expiry; revocation cascades to driver unbind and emits `DEVICE_DISCONNECTED`. An operator may explicitly revoke a grant via `RevokeRemovableDeviceApproval`; this transitions the device to `QUARANTINED` with reason `OPERATOR_INITIATED`.

## 9. Adversarial robustness

### 9.1 Device-id spoofing

A malicious USB device claims `vendor_id = 0x046d` (Logitech) and `device_id_pci_usb = 0xc52b` (Unifying Receiver) but its descriptor structure does not match Logitech's known fingerprint. Mitigation: the AIOS hardware database (§5.2) contains a fingerprint match check; descriptor mismatch produces `CAPABILITY_LIE` quarantine reason and emits `HOST_CAPABILITY_LIE` evidence (`FOREVER`). The device parks at `QUARANTINED`; the operator decides whether to investigate or retire it. This generalises S8.2 §8 from GPU to all device classes per I11.

### 9.2 Firmware version downgrade

An attacker (or buggy firmware-update process) attempts to install firmware version `1.2` on a device whose previous boot recorded `firmware_version = 1.5`. Mitigation per I13: the cross-boot diff catches the downgrade (mutated entry, lower `firmware_version`); the device is parked at `QUARANTINED` with reason `FIRMWARE_DOWNGRADE`; `FIRMWARE_VERSION_DOWNGRADE_BLOCKED` evidence is emitted (`FOREVER`). The operator may, in recovery mode with a hardware-key co-signer, override (e.g., legitimate vendor rollback for a defective firmware). The override path itself is FOREVER-evidenced. Cite S8.4 for the firmware-trust monotonicity rule that makes downgrade an exception rather than the norm.

### 9.3 USB-borne attacks (BadUSB, juicing)

A "juice-jacking" cable or a BadUSB device is plugged into a host port. Mitigation: the `RemovableDevicePolicy.REQUIRE_APPROVAL` default (I12) holds the device at `IDENTIFIED` until a HUMAN_USER explicitly approves. The operator sees the typed approval surface naming the device's class, vendor, model, and any anomalous fingerprint signal (a USB device that claims to be a HID keyboard but whose descriptor sequence triggers a known BadUSB pattern is flagged in the approval surface). For air-gapped postures, the operator may set `OPERATOR_AIRGAP` for `USB_DEVICE`, which permanently denies USB device admission outside of an explicit physical key-swap operation in recovery.

### 9.4 Thunderbolt DMA attack

A malicious Thunderbolt dock attempts to exercise direct memory access against host RAM. Mitigation per I14: Thunderbolt and USB4 admission **requires** IOMMU enforcement. If the host's IOMMU is disabled (BIOS/firmware setting), the entire Thunderbolt subtree is parked at `QUARANTINED` and emits `IOMMU_DMA_PROTECTION_DEGRADED` (`EXTENDED_60M`). For `PCIE` devices, IOMMU absence yields the same evidence record but admission is allowed at `DEGRADED` (the constitutional position is that PCIe is internal hardware and a coffee-shop attacker is unlikely to attack a soldered-on NIC; Thunderbolt is the genuinely external DMA surface). Cite S8.2 §5.3 for the analogous IOMMU degradation discipline at the GPU layer.

### 9.5 Evil-maid hardware swap

An attacker with brief physical access swaps the host's Wi-Fi card with a malicious clone (same vendor/device id, different serial, modified firmware). Mitigation per I6: cross-boot drift detection (§7) catches the changed identity tuple — even if vendor/device id match, the `serial_number` differs (or, if the attacker preserves the serial, the `firmware_version` likely differs because the clone runs different firmware). The diff produces an unapproved drift entry; `HARDWARE_GRAPH_DRIFT_DETECTED` evidence is emitted (`FOREVER`); the operator-alert is raised. Even if the attacker engineers a clone that perfectly matches all identifiable fields, the `capabilities_observed` will likely diverge from the previous boot's record (driver feature flags differ across firmware revisions), producing a mutated entry. The discipline is **forensic, not absolute**: an attacker with sufficient resources to perfectly clone a device including firmware will not be caught by the graph; AIOS does not claim to defeat nation-state hardware substitution. The discipline raises the cost of opportunistic evil-maid attacks to a level inconsistent with coffee-shop access.

### 9.6 Driver-injection attack

An attacker convinces the host to load an out-of-tree malicious driver. Mitigation per I10: the driver-binding evaluation refuses out-of-tree drivers by default (`OUT_OF_TREE_BLACKLISTED` trust class). The override path requires recovery-mode operator co-signer plus FOREVER evidence; a normal-mode `ModprobeRequest` (from any subject including AI) is hard-denied. The S2.3 hard-deny `AISystemAdminBlocked` fires for AI-initiated attempts; non-AI normal-mode attempts hit the `RECOVERY_REQUIRED` error code per INV-012.

### 9.7 Supply-chain firmware tampering

An attacker compromises a vendor's firmware update path and ships a tampered firmware blob. This is **outside** the HDM's defence perimeter; it is S8.4's concern (firmware signature, vendor key pinning, version monotonicity). The HDM reflects S8.4's verdict via `Device.firmware_trusted` and `DeviceTrustClass.FIRMWARE_TRUSTED`. A device whose firmware fails S8.4 verification is parked at `QUARANTINED` with reason `CAPABILITY_LIE` (or `FIRMWARE_DOWNGRADE` if the issue is monotonicity). The HDM does **not** independently verify firmware signatures; it consumes S8.4's verdict.

## 10. Performance contract

| Operation                                                  | p50      | p95      | p99      | Hard timeout |
| ---------------------------------------------------------- | -------- | -------- | -------- | ------------ |
| Cold-boot graph build (first 100 devices)                  | < 500 ms | < 2 s    | < 5 s    | 30 s         |
| Hot-plug single-device update                              | < 50 ms  | < 250 ms | < 1 s    | 5 s          |
| Cross-boot drift diff (≤ 100 devices each side)            | < 20 ms  | < 100 ms | < 250 ms | 1 s          |
| `ApproveRemovableDevice`                                   | < 10 ms  | < 50 ms  | < 200 ms | 1 s          |
| `GetActiveSnapshot` (cache hit)                            | < 100 µs | < 1 ms   | < 5 ms   | 50 ms        |
| `GetSnapshot(snapshot_id)` (cold; AIOS-FS read)            | < 5 ms   | < 25 ms  | < 100 ms | 500 ms       |
| Capability advertised-vs-observed probe (per device)       | < 10 ms  | < 50 ms  | < 200 ms | 1 s          |
| Driver bind evaluation + bind (typical kernel module load) | < 50 ms  | < 500 ms | < 2 s    | 10 s         |

Failure modes — all fail closed:

- `GraphNotInitialized` — pre-S9.2 STAGE_RUNTIME_SERVICES_STARTED; HDM is not yet running.
- `GraphSignatureInvalid` — caller presented a forged or corrupted graph; consumer rejects.
- `DeviceUnclassifiable` — kernel attributes do not match any class; device parks at `IDENTIFIED` under `OTHER`.
- `DriverBindRejected` — no acceptable driver candidate; device parks at `QUARANTINED`.
- `RemovableDeviceDenied` — approval refused (AI subject, recovery-deny, policy-deny).
- `FirmwareDowngradeDetected` — I13.
- `IommuRequiredAbsent` — I14 on Thunderbolt/USB4.
- `DriftWithoutApproval` — cross-boot drift not approved; FOREVER evidence emitted.
- `HostCapabilityLie` — advertised vs. observed mismatch; FOREVER evidence emitted.
- `AiForbiddenOperation` — AI subject attempted a mutating HDM RPC; INV-013 fires; FOREVER evidence emitted.
- `RecoveryRequired` — operation requires recovery mode; INV-012 cites.
- `PolicyRefused` — generic policy bundle deny.

## 11. Cross-spec dependencies

| Spec                       | Direction | What this spec contributes / consumes                                                                                                                                                                                                                                                                                                                                                                  |
| -------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| S0.1                       | producer  | Action envelopes targeting hardware (`approve_removable_device`, `quarantine_device`, `rebind_driver`) carry `target.device_id`, `target.device_class`, `target.device_trust_class`, `target.removable`, `target.driver_provenance`, `target.firmware_trusted`. Five new closed condition fields are queued for S2.3.                                                                                  |
| S1.3                       | consumer  | The `HardwareGraph` is stored as a content-addressed AIOS-FS object at `/aios/system/hardware/snapshots/<hwgraph_id>`. The active and previous snapshots are referenced by AIOS-FS path.                                                                                                                                                                                                               |
| S2.3                       | producer  | Five closed condition fields queued (touch-up): `target.device_class`, `target.device_trust_class`, `target.removable`, `target.driver_provenance`, `target.firmware_trusted`. The constitutional hard-deny `AISystemAdminBlocked` already fires for `/aios/system/hardware/...` mutations per INV-013; this spec confirms the binding.                                                                |
| S3.1                       | producer  | Fifteen new record types (§12) added to the closed `RecordType` enum and the retention-class table — fourteen at Wave 8 close, plus `HARDWARE_GRAPH_DRIFT_ACCEPTED` (FOREVER) added by Wave 9 Cluster 8 finding SIM-C-002 to pair with the existing `HARDWARE_GRAPH_DRIFT_DETECTED` so the audit chain records both detection and resolution. Queued for S3.1 W10.                                     |
| S3.2                       | producer  | New field `device_policy: DevicePolicy` on `SandboxProfile`, where `DevicePolicy { max_device_class_set, deny_removable, allow_thunderbolt }` constrains the device classes a sandboxed process may interact with via syscall (e.g., `mount(2)`, `open("/dev/...")`). The sandbox cannot be broader than the per-subject removable-device grant; intersection is enforced. Cross-spec touch-up queued. |
| S4.1                       | consumer  | `/aios/system/hardware/active`, `/aios/system/hardware/previous`, `/aios/system/hardware/snapshots/<id>`, `/aios/system/hardware/grants/<id>`, `/aios/system/hardware/db` all reside in `_system` scope; AI subjects have no read access except for graph view via the HDM read-only RPCs (which return a redacted projection: device classes and counts but not serial numbers or MAC addresses).     |
| S5.1                       | consumer  | `subject.is_ai` drives I8 hard-deny. `subject.recovery_mode` drives I7 (recovery removable default). The HDM signing subject is `_system:service:hardware-manager`.                                                                                                                                                                                                                                    |
| S5.2                       | consumer  | The HDM signing key (`hardware.graph.sign`) is held in the Vault Broker; the HDM service is the sole subject capable of producing graph signatures.                                                                                                                                                                                                                                                    |
| S8.1                       | producer  | `DeviceClass.NETWORK_ETHERNET`, `NETWORK_WIFI`, `NETWORK_BLUETOOTH` populate the network-policy enforcer's interface set. Network-policy `LAN_LOCAL` posture binds to the active set of network adapters; an unapproved Wi-Fi adapter cannot serve as the egress interface for an outbound grant.                                                                                                      |
| S8.2                       | producer  | `DeviceClass.GPU_INTEGRATED` and `GPU_DISCRETE` are the entries S8.2 consumes for `GpuDevice` topology. The HardwareGraph carries the device-level fields (vendor, model, bus path, `iommu_group_id`); S8.2's `GpuDevice` extends with VRAM and queue-priority semantics. The capability-lie discipline of this spec extends S8.2 §8 to all device classes.                                            |
| S8.4                       | consumer  | `Device.firmware_version` and `Device.firmware_trusted` are the HDM-side reflections of S8.4's signed-firmware-blob verdict. S8.4 owns the verification; the HDM owns the version-monotonic check across boots (I13) and the parking decision.                                                                                                                                                         |
| S9.1                       | consumer  | `recovery_mode` drives I7 default policy. Recovery-mode boots park removable devices at `DENY_DEFAULT`; the constitutional roots `/`, `/root`, `/aios` (per INV-004) constrain HDM read/write paths.                                                                                                                                                                                                   |
| S9.2                       | consumer  | First-boot `STAGE_RUNTIME_SERVICES_STARTED` triggers the first hardware-graph build. The resulting `hwgraph_<...>` enters the first-boot marker hash composition.                                                                                                                                                                                                                                      |
| S9.3                       | producer  | The active `hardware_graph_snapshot_id` is the canonical input to `BuildRequest.hardware_graph_snapshot_id` for dedicated-kernel builds. A drift-affected graph **must not** be used as input until the operator approves the drift; an attempt to start a kernel build with an unapproved-drift snapshot is refused with `DriftWithoutApproval`.                                                      |
| S11.1                      | consumer  | Firmware blobs referenced by the graph are sourced as signed packages from the AIOS repository. `DriverProvenance.FIRMWARE_BUNDLE` references S11.1 package identifiers.                                                                                                                                                                                                                               |
| L0 INV-007 (downward deps) | enforcer  | This spec lives in L8 and depends only on L0–L7 contracts plus L8's siblings (S8.1, S8.2, S8.4) and the L9 evidence log; no upward dependency.                                                                                                                                                                                                                                                         |

### 11.1 Cross-spec touch-ups queued

The following cannot be applied within this spec without violating one-spec-one-contract; they are flagged for the next consolidation cycle:

- **S2.3** to add five closed condition fields: `target.device_class`, `target.device_trust_class`, `target.removable`, `target.driver_provenance`, `target.firmware_trusted`. Constitutional hard-deny `RemovableDeviceForbiddenForAI` is **already covered** by `AISystemAdminBlocked`; no new hard-deny is required.
- **S3.1** to absorb the 15 new record types (§12) into the closed `RecordType` enum and the retention-class table (14 at Wave 8 close + `HARDWARE_GRAPH_DRIFT_ACCEPTED` FOREVER from Wave 9, Cluster 8 SIM-C-002).
- **S3.2** to add `device_policy: DevicePolicy` field on `SandboxProfile` and the apply-time check that the sandbox's permitted device-class set intersects with the per-subject grant.
- **L0 invariant candidate** `HARDWARE_GRAPH_DRIFT_FOREVER`: every unapproved cross-boot graph drift emits FOREVER evidence. Queued for L0 consolidation.

## 12. Evidence record types queued

Fifteen records added to the closed `RecordType` enum (S3.1) — fourteen at Wave 8 close, one added in Wave 9 (Cluster 8 finding SIM-C-002 — `HARDWARE_GRAPH_DRIFT_ACCEPTED`):

| Record type                          | Retention      | Carries                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ------------------------------------ | -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `HARDWARE_GRAPH_REBUILT`             | `STANDARD_24M` | `snapshot_id`, `previous_snapshot_id`, `device_count`, `built_at`, `recovery_mode`, signed by `_system:service:hardware-manager`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `DEVICE_DETECTED`                    | `STANDARD_24M` | First-time observation of a device-identity tuple; carries `device_id`, `class`, `vendor_id`, `device_id_pci_usb`, `bus_kind`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `DEVICE_DRIVER_BOUND`                | `STANDARD_24M` | Successful driver bind; carries `device_id`, `driver_id`, `driver_provenance`, `trust_class`, `firmware_trusted`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `DEVICE_DRIVER_REJECTED`             | `EXTENDED_60M` | Bind refused; carries `device_id`, candidate `driver_id`, `provenance`, refusal reason                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `DEVICE_QUARANTINED`                 | `FOREVER`      | Lifecycle transition into `QUARANTINED`; carries `device_id`, `reason` (`DeviceQuarantineReason`), prior state                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `DEVICE_DISCONNECTED`                | `STANDARD_24M` | Hot-unplug or hard-disconnect; carries `device_id`, `class`, last `lifecycle_state` before disconnect                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `REMOVABLE_DEVICE_REQUEST`           | `STANDARD_24M` | A removable device awaits approval; carries `device_id`, `class`, requesting subject id, model_string                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `REMOVABLE_DEVICE_APPROVED`          | `STANDARD_24M` | Approval granted; carries `device_id`, approving subject id, policy class (`ALLOW_AUTO_THIS_BOOT` / `FOREVER` / per-group), TTL                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `REMOVABLE_DEVICE_DENIED`            | `EXTENDED_60M` | Approval refused; carries `device_id`, denying subject id, refusal reason (recovery-deny, policy-deny, AI-blocked)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `AI_REMOVABLE_DEVICE_BLOCKED`        | `FOREVER`      | INV-013 device-plane hard-deny fired; carries AI subject id, attempted RPC name, `device_id`, action envelope id                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `HARDWARE_GRAPH_DRIFT_DETECTED`      | `FOREVER`      | I6 cross-boot drift; carries `prior_snapshot_id`, `current_snapshot_id`, added/removed/mutated sets, operator-alert flag                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `FIRMWARE_VERSION_DOWNGRADE_BLOCKED` | `FOREVER`      | I13 monotonicity violation; carries `device_id`, prior `firmware_version`, attempted `firmware_version`, source                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `IOMMU_DMA_PROTECTION_DEGRADED`      | `EXTENDED_60M` | I14 degradation event; carries `device_id`, `bus_kind`, host IOMMU global state                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `OUT_OF_TREE_DRIVER_BLOCKED`         | `FOREVER`      | I10 out-of-tree driver bind refused; carries `device_id`, attempted `driver_id`, source. (Note: a successful out-of-tree admission under recovery override is logged separately as `DEVICE_DRIVER_BOUND` with provenance recorded.)                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `HARDWARE_GRAPH_DRIFT_ACCEPTED`      | `FOREVER`      | **W9 addition** (Cluster 8, finding SIM-C-002). Operator (`hardware.accept_drift_accessory` HUMAN_USER) or recovery-mode operator (`hardware.accept_drift_substrate` RECOVERY_ONLY) accepted a previously-detected hardware-graph drift. Carries: `prior_snapshot_id`, `current_snapshot_id`, `accepted_device_ids[]`, `originating_drift_record_id` (link to the `HARDWARE_GRAPH_DRIFT_DETECTED` event), `operator_subject_canonical_id`, `is_substrate: bool`, `is_recovery_mode: bool`. Constitutional forensic event — pairs with the FOREVER detection record so the audit chain shows both detection and resolution. Queued for S3.1 W10 cross-spec consolidation. |

After this addition (post-W9) the `RecordType` vocabulary grows by 15 entries; FOREVER count grows by 6 (the deliberately small set of constitutional and forensic events: quarantine, AI-block, drift-detected, drift-accepted, downgrade, out-of-tree). The previously-narrated "6 FOREVER" claim was self-flagged at W8.4.4 as a 6-vs-5 mismatch against the §12 table; W9 closes the mismatch by adding the matching accept-side record (`HARDWARE_GRAPH_DRIFT_ACCEPTED`), bringing the actual FOREVER tally to 6 and the total queued vocabulary to 15.

## 13. Telemetry contract

All metrics MUST use bounded label cardinality. **`device_id`, `subject_canonical_id`, `serial_number`, `mac_address`, `bus_path` are NEVER labels.** `device_class`, `bus_kind`, `trust_class`, `lifecycle_state`, `driver_provenance`, `removable_policy` are bounded.

| Metric                                | Type    | Labels (closed)                                               |
| ------------------------------------- | ------- | ------------------------------------------------------------- |
| `hardware_graph_rebuild_total`        | counter | `recovery_mode` (true/false)                                  |
| `hardware_graph_active_devices`       | gauge   | `device_class`, `lifecycle_state`                             |
| `hardware_device_detection_total`     | counter | `device_class`, `bus_kind`                                    |
| `hardware_device_driver_bind_total`   | counter | `device_class`, `driver_provenance`, `result` (success/error) |
| `hardware_device_quarantine_total`    | counter | `device_class`, `reason`                                      |
| `hardware_removable_request_total`    | counter | `device_class`                                                |
| `hardware_removable_approval_total`   | counter | `device_class`, `result` (approved/denied)                    |
| `hardware_removable_ai_blocked_total` | counter | `device_class`                                                |
| `hardware_graph_drift_total`          | counter | `kind` (added/removed/mutated)                                |
| `hardware_firmware_downgrade_total`   | counter | `device_class`                                                |
| `hardware_iommu_degraded_total`       | counter | `bus_kind`                                                    |
| `hardware_capability_lie_total`       | counter | `device_class`                                                |
| `hardware_out_of_tree_blocked_total`  | counter | `device_class`                                                |
| `hardware_lifecycle_transition_total` | counter | `from_state`, `to_state`                                      |

Cardinality budget: ≤ 200 active label tuples per metric. With 16 device classes × 8 lifecycle states the worst-case `hardware_graph_active_devices` cardinality is 128 — within budget.

## 14. Worked examples

### Example 1 — Boot-time hardware graph build

```text
Setup:
  Host has just completed S9.2 STAGE_RUNTIME_SERVICES_STARTED.
  Hardware: 1 CPU, 1 GPU_INTEGRATED, 1 GPU_DISCRETE (NVIDIA RTX 4070),
            1 DISK_NVME (Samsung 990 Pro), 1 NETWORK_ETHERNET, 1 NETWORK_WIFI,
            1 NETWORK_BLUETOOTH, 1 AUDIO_OUTPUT, 1 AUDIO_INPUT, 1 USB_HUB (root),
            1 TPM_2_0, 1 SECURITY_KEY (FIDO2 dongle in port).
  IOMMU: enforced globally.
  Previous snapshot: empty (first boot after S9.2 first-boot completed without Thunderbolt
                     hot-plug events).

HDM enumerates udev events (coldplug):
  Each event → classification → driver-bind evaluation → registration.
  GPU_DISCRETE: AIOS_VERIFIED_DRIVER not present for RTX 4070 in v1; falls back to
                SIGNED_KERNEL_DRIVER (nvidia kernel module signed by upstream).
                trust_class = SIGNED_KERNEL_DRIVER, lifecycle_state = ACTIVE.
  NETWORK_WIFI: AIOS_VERIFIED_DRIVER present for Intel AX210 (curated).
                trust_class = AIOS_VERIFIED_DRIVER, lifecycle_state = ACTIVE.
  SECURITY_KEY: removable, but RemovableDevicePolicy = REQUIRE_APPROVAL.
                Held at IDENTIFIED. REMOVABLE_DEVICE_REQUEST evidence emitted.
                Operator approves with TTL=24h, policy class ALLOW_AUTO_THIS_BOOT.
                Transitions IDENTIFIED → DRIVER_BOUND → ACTIVE.

Graph build completes:
  device_count = 12
  snapshot_id  = "hwgraph_a3f5e1c2d8b6794052ef317c9a2b1d8e"
  previous_snapshot_id = ""  (no prior snapshot on this boot path)
  signed by _system:service:hardware-manager.

Evidence:
  HARDWARE_GRAPH_REBUILT (STANDARD_24M)
  12 × DEVICE_DETECTED (STANDARD_24M, first-time observations)
  12 × DEVICE_DRIVER_BOUND (STANDARD_24M)
  1 × REMOVABLE_DEVICE_REQUEST (STANDARD_24M, for FIDO2 dongle)
  1 × REMOVABLE_DEVICE_APPROVED (STANDARD_24M)

Snapshot persisted at /aios/system/hardware/active.
S9.3 may now read the snapshot id as input to a kernel build.
```

### Example 2 — USB device approval flow with AI block attempt

```text
Setup:
  Host is in normal mode. RemovableDevicePolicy default = REQUIRE_APPROVAL.
  Active subjects: family:alice (HUMAN_USER, primary_group=family),
                   family:agent:helper (AI, is_ai=true).
  Operator plugs in a USB mass-storage device (16 GB, vendor "SanDisk", model "Cruzer").

HDM observes udev event:
  Classifies as USB_DEVICE.
  Evaluates RemovableDevicePolicy: REQUIRE_APPROVAL.
  Holds device at IDENTIFIED.
  Emits REMOVABLE_DEVICE_REQUEST (STANDARD_24M) carrying device_id, class, vendor, model.

AI agent family:agent:helper observes the surface and attempts to call:
  ApproveRemovableDevice(device_id="dev_01HZ...", policy_class=ALLOW_AUTO_THIS_BOOT)

HDM enforcement:
  Subject resolution: family:agent:helper has subject.is_ai = true.
  S2.3 hard-deny AISystemAdminBlocked fires — target resolves to /aios/system/hardware/...
  RPC returns AI_FORBIDDEN_OPERATION.
  Emits AI_REMOVABLE_DEVICE_BLOCKED (FOREVER) carrying:
    ai_subject_id: "family:agent:helper"
    attempted_rpc: "ApproveRemovableDevice"
    device_id: "dev_01HZ..."
    action_envelope_id: "act_<...>"

family:alice (HUMAN_USER) reviews the surface:
  Calls ApproveRemovableDevice(device_id="dev_01HZ...", policy_class=ALLOW_AUTO_THIS_BOOT,
                                ttl=24h).

HDM enforcement:
  Subject resolution: family:alice is HUMAN_USER. Pass.
  IOMMU posture: USB3 not DMA-capable in the protected sense; pass.
  S2.3 policy bundle: no deny.
  Driver bind: USB mass-storage class driver, SIGNED_KERNEL_DRIVER provenance.
  Lifecycle: IDENTIFIED → DRIVER_BOUND → ACTIVE.
  Emits:
    REMOVABLE_DEVICE_APPROVED (STANDARD_24M)
    DEVICE_DRIVER_BOUND (STANDARD_24M)

The device is now mountable by family:alice (subject to S3.2 sandbox device_policy).
The AI block attempt is permanently recorded.
```

### Example 3 — Evil-maid hardware swap detection

```text
Setup:
  Host's previous boot recorded:
    Device "dev_01H..." in slot Wi-Fi M.2:
      class = NETWORK_WIFI
      vendor_id = "0x8086", device_id_pci_usb = "0x51f0"
      serial_number = "WFI-A1B2C3D4"
      firmware_version = "62.18.5.0"
      capabilities_observed = { "standards": "802.11be", "bands": "2.4/5/6" }
      snapshot_id_recorded_in: "hwgraph_<prev_hex32>"

  Attacker, during a coffee-shop window, replaces the M.2 Wi-Fi card with a clone:
    same vendor_id and device_id_pci_usb (visible),
    different serial_number "WFI-X9Y8Z7W6",
    different firmware_version "63.01.0.0" (a custom build),
    different capabilities_observed (subtly modified driver feature flags).

Boot N+1:
  HDM enumerates devices. The slot reports:
    serial_number = "WFI-X9Y8Z7W6"
    firmware_version = "63.01.0.0"
    capabilities_observed = { "standards": "802.11be", "bands": "2.4/5",
                              "feature_flags": "<modified>" }

  HDM runs cross-boot diff (§7.1) against previous snapshot:
    Identity tuple key (vendor_id, device_id_pci_usb, serial_number) differs.
    Diff result:
      removed: [ {device_id: "dev_01H...", serial: "WFI-A1B2C3D4"} ]
      added:   [ {device_id: "dev_02K...", serial: "WFI-X9Y8Z7W6"} ]
    No contemporaneous DEVICE_DISCONNECTED record exists for the removed serial.
    No REMOVABLE_DEVICE_APPROVED record exists for the added serial.
    Drift is unapproved.

HDM enforcement:
  Emits HARDWARE_GRAPH_DRIFT_DETECTED (FOREVER) carrying:
    prior_snapshot_id: "hwgraph_<prev_hex32>"
    current_snapshot_id: "hwgraph_<curr_hex32>"
    added: [ {class: NETWORK_WIFI, vendor: "0x8086:0x51f0", serial: "WFI-X9Y8Z7W6"} ]
    removed: [ {class: NETWORK_WIFI, vendor: "0x8086:0x51f0", serial: "WFI-A1B2C3D4"} ]
    mutated: []
    operator_alert_raised: true
  Emits FIRMWARE_VERSION_DOWNGRADE_BLOCKED? — No, version is higher (63 > 62), but
  the firmware_trusted flag depends on S8.4 verification of the new blob.
  S8.4 verifies the firmware blob against the vendor catalogue:
    The firmware "63.01.0.0" is NOT in the signed Intel catalogue.
    S8.4 returns firmware_trusted = false.
  HDM parks the new device at QUARANTINED with reason CAPABILITY_LIE.
  Emits DEVICE_QUARANTINED (FOREVER).

S9.3 dedicated kernel pipeline:
  Refuses to start a build with this snapshot id; returns DriftWithoutApproval.
  The operator must enter recovery mode, review the drift, and decide:
    (a) accept the drift (legitimate hardware repair) → hardware.accept_drift_accessory
        (HUMAN_USER, normal mode) for accessory devices, or hardware.accept_drift_substrate
        (RECOVERY_ONLY, hardware-key co-signer) for CPU/TPM/BIOS_UEFI/firmware-bound GPU;
        emits HARDWARE_GRAPH_DRIFT_ACCEPTED (FOREVER) carrying originating_drift_record_id
        linking to the prior HARDWARE_GRAPH_DRIFT_DETECTED record (W9 Cluster 8 SIM-C-002)
    (b) reject and retire the new device → operator removes the swapped card, restores original
    (c) investigate (compromise suspected) → host stays in degraded session posture.

The constitutional record is durable: even if the operator accepts (a), the FOREVER
HARDWARE_GRAPH_DRIFT_DETECTED record remains. The audit chain shows the swap, the alert,
the operator's response, and the resolution.
```

## 15. Acceptance criteria

- [ ] `DeviceClass` is a closed enum with 16 values plus `OTHER` plus `_UNSPECIFIED` zero (§3.1).
- [ ] `DeviceTrustClass` is a closed enum with 5 values (§3.2).
- [ ] `DeviceLifecycleState` is a closed enum with 8 values (§3.3); allowed transitions match §6.1.
- [ ] `RemovableDevicePolicy` is a closed enum with 5 values (§3.4); default = `REQUIRE_APPROVAL`; recovery default = `DENY_DEFAULT`.
- [ ] `BusKind` is a closed enum with 8 values (§3.5).
- [ ] `DriverProvenance` is a closed enum with 5 values (§3.6).
- [ ] `DeviceQuarantineReason` is a closed enum with 8 values (§3.7).
- [ ] `HardwareGraphErrorCode` is a closed enum (§3.8).
- [ ] `HardwareGraph.snapshot_id` is `hwgraph_<hex_lower(BLAKE3(JCS(graph)))[:32]>`.
- [ ] Every `HardwareGraph` carries an Ed25519 signature from `_system:service:hardware-manager`; consumers verify before trust.
- [ ] Hardware graph is rebuilt at every boot; the previous snapshot id is retained for diff.
- [ ] Cross-boot diff identifies added / removed / mutated devices keyed by identity tuple `(vendor_id, device_id_pci_usb, serial_number)`.
- [ ] Unapproved drift emits `HARDWARE_GRAPH_DRIFT_DETECTED` (FOREVER) with operator alert.
- [ ] AI subjects cannot mount or approve removable devices (INV-013); attempts emit `AI_REMOVABLE_DEVICE_BLOCKED` (FOREVER).
- [ ] Recovery-mode boots default removable policy to `DENY_DEFAULT` (INV-004 binding).
- [ ] Out-of-tree drivers refuse to bind by default; admission requires recovery + co-signer + FOREVER evidence.
- [ ] Capability advertised vs. observed mismatch parks device at `QUARANTINED` with `CAPABILITY_LIE` reason; `HOST_CAPABILITY_LIE` evidence (FOREVER).
- [ ] Firmware version downgrade detected via cross-boot comparison; `FIRMWARE_VERSION_DOWNGRADE_BLOCKED` evidence (FOREVER).
- [ ] Thunderbolt/USB4 admission requires IOMMU enforcement; absent IOMMU parks at `QUARANTINED` with `IOMMU_REQUIRED_ABSENT` reason.
- [ ] PCIe IOMMU absence emits `IOMMU_DMA_PROTECTION_DEGRADED` (`EXTENDED_60M`) but admits at `DEGRADED`.
- [ ] All 15 evidence record types of §12 emitted at the correct retention class (14 at Wave 8 close + `HARDWARE_GRAPH_DRIFT_ACCEPTED` from W9 Cluster 8).
- [ ] All three worked examples (§14) produce the specified outcomes.
- [ ] Telemetry conforms to §13; device id / serial / MAC / bus path never appear as labels.
- [ ] L0 INV-007 layer-downward dependency satisfied: this spec depends only on L0–L8 contracts (S0.1, S1.3, S2.3, S3.1, S3.2, S4.1, S5.1, S5.2, S8.1, S8.2, S8.4, S9.1, S9.2, S9.3, S11.1) — no upward dependency.

## 16. Open deferrals

- **AIOS verified driver registry contents** — the curated list of `AIOS_VERIFIED_DRIVER` candidates is itself a deferred artefact; this spec defines the trust class but the population mechanism is queued for an L8 sub-spec or a distribution-layer (S11.x) deliverable.
- **Per-bus quotas** — limits on the count of admitted devices per `BusKind` (e.g., maximum 8 USB devices per host) to bound resource exhaustion. Deferred.
- **Sensor-layer subscription model** — `SENSOR_TEMPERATURE` and `SENSOR_BATTERY` produce streams of values; the subscription/sampling model belongs to a future L9 telemetry spec.
- **Hot-plug debouncing** — the 250 ms debounce in §5.5 is a placeholder. The exact algorithm (single-window debounce vs. exponential backoff vs. event coalescing) is deferred.
- **Multi-host hardware graph federation** — for AIOS clusters spanning multiple hosts, a federated hardware-graph view. Deferred to a future distributed AIOS spec.
- **Predictive failure** — using `SENSOR_TEMPERATURE` and `SMART` data to predict imminent disk failure and pre-emptively transition `ACTIVE → DEGRADED`. Deferred.
- **Operator-configurable per-class removable defaults via policy bundles** — the §3.4 defaults are this spec's floors; the override mechanism is queued for the S2.3 / policy-bundle touch-up.
- **Fingerprint database extension protocol** — how new vendor/device-id fingerprint entries are added to the AIOS hardware database (§5.2) without a full host firmware update. Deferred.
- **Cross-graph chain-of-custody verification** — verifying that the snapshot chain `prev → curr → next` is unbroken across host lifetime, beyond the per-pair diff of §7. Deferred.
- **GPU sub-device nodes (MIG, SR-IOV partitions)** — the HDM tracks the physical GPU as one device; sub-partitions belong to S8.2's deferred SR-IOV/MIG work.

## 17. See also

- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S1.3 — AIOS-FS Object Model](../L2_AIOS_FS/01_object_model.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S3.2 — Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [S4.1 — Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S5.1 — Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [S8.1 — Network Policy](02_network_policy.md)
- [S8.2 — GPU Resource Model](05_gpu_resource_model.md)
- [S8.4 — Firmware Trust](04_firmware_trust.md)
- [S9.1 — Recovery Boundary](../L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md)
- [S9.2 — First-Boot Flow](../L1_Kernel_Bootstrap_Recovery/02_first_boot_flow.md)
- [S9.3 — Dedicated Kernel Pipeline](../L1_Kernel_Bootstrap_Recovery/03_dedicated_kernel_pipeline.md)
- [S11.1 — Repository Model](../L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [L0 INV-004 Recovery boundary](../L0_Governance_Evidence_Safety/04_invariants.md)
- [L0 INV-013 AI cannot perform system admin](../L0_Governance_Evidence_Safety/04_invariants.md)
- [L0 INV-024 GPU compute capability-gated](../L0_Governance_Evidence_Safety/04_invariants.md)
- [L8 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

## Appendix A — Full Proto IDL

```proto
syntax = "proto3";
package aios.hardware.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/duration.proto";

// ============================================================================
// Service
// ============================================================================

service HardwareManagerService {
  // Read RPCs (allowed for AI subjects subject to redaction projection).
  rpc GetActiveSnapshot(GetActiveSnapshotRequest) returns (GetActiveSnapshotResponse);
  rpc GetSnapshot(GetSnapshotRequest) returns (GetSnapshotResponse);
  rpc ListDevices(ListDevicesRequest) returns (ListDevicesResponse);
  rpc GetDevice(GetDeviceRequest) returns (GetDeviceResponse);
  rpc ProbeDeviceCapabilities(ProbeDeviceCapabilitiesRequest)
      returns (ProbeDeviceCapabilitiesResponse);

  // Mutating RPCs (HUMAN_USER only; AI subjects hard-denied via INV-013).
  rpc ApproveRemovableDevice(ApproveRemovableDeviceRequest)
      returns (ApproveRemovableDeviceResponse);
  rpc RevokeRemovableDeviceApproval(RevokeRemovableDeviceApprovalRequest)
      returns (RevokeRemovableDeviceApprovalResponse);
  rpc QuarantineDevice(QuarantineDeviceRequest) returns (QuarantineDeviceResponse);
  rpc ResumeDevice(ResumeDeviceRequest) returns (ResumeDeviceResponse);
  rpc RebindDriver(RebindDriverRequest) returns (RebindDriverResponse);
  rpc AcceptDrift(AcceptDriftRequest) returns (AcceptDriftResponse);
  rpc RetireDevice(RetireDeviceRequest) returns (RetireDeviceResponse);

  // Read-only diagnostic.
  rpc DiffSnapshots(DiffSnapshotsRequest) returns (DiffSnapshotsResponse);
}

// ============================================================================
// Closed enums (re-declared here for the IDL bundle)
// ============================================================================

enum DeviceClass {
  DEVICE_CLASS_UNSPECIFIED = 0;
  CPU                      = 1;
  GPU_INTEGRATED           = 2;
  GPU_DISCRETE             = 3;
  DISK_NVME                = 4;
  DISK_SATA                = 5;
  NETWORK_ETHERNET         = 6;
  NETWORK_WIFI             = 7;
  NETWORK_BLUETOOTH        = 8;
  AUDIO_OUTPUT             = 9;
  AUDIO_INPUT              = 10;
  USB_HUB                  = 11;
  USB_DEVICE               = 12;
  THUNDERBOLT_HOST         = 13;
  SENSOR_TEMPERATURE       = 14;
  SENSOR_BATTERY           = 15;
  TPM_2_0                  = 16;
  SECURITY_KEY             = 17;
  OTHER                    = 18;
}

enum DeviceTrustClass {
  DEVICE_TRUST_CLASS_UNSPECIFIED = 0;
  AIOS_VERIFIED_DRIVER           = 1;
  SIGNED_KERNEL_DRIVER           = 2;
  DISTRO_PROVIDED_DRIVER         = 3;
  OUT_OF_TREE_BLACKLISTED        = 4;
  FIRMWARE_TRUSTED               = 5;
}

enum DeviceLifecycleState {
  DEVICE_LIFECYCLE_STATE_UNSPECIFIED = 0;
  DETECTED                           = 1;
  IDENTIFIED                         = 2;
  DRIVER_BOUND                       = 3;
  ACTIVE                             = 4;
  DEGRADED                           = 5;
  DISCONNECTED                       = 6;
  QUARANTINED                        = 7;
  RETIRED                            = 8;
}

enum RemovableDevicePolicy {
  REMOVABLE_DEVICE_POLICY_UNSPECIFIED = 0;
  ALLOW_AUTO                          = 1;
  REQUIRE_APPROVAL                    = 2;
  DENY_DEFAULT                        = 3;
  RECOVERY_ONLY                       = 4;
  OPERATOR_AIRGAP                     = 5;
}

enum BusKind {
  BUS_KIND_UNSPECIFIED = 0;
  PCIE                 = 1;
  USB2                 = 2;
  USB3                 = 3;
  USB4                 = 4;
  THUNDERBOLT          = 5;
  SATA                 = 6;
  NVME_BUS             = 7;
  PLATFORM             = 8;
}

enum DriverProvenance {
  DRIVER_PROVENANCE_UNSPECIFIED = 0;
  AIOS_REGISTRY                 = 1;
  KERNEL_MAINLINE               = 2;
  DISTRO_PACKAGE                = 3;
  FIRMWARE_BUNDLE               = 4;
  OUT_OF_TREE_REJECTED          = 5;
}

enum DeviceQuarantineReason {
  DEVICE_QUARANTINE_REASON_UNSPECIFIED = 0;
  CAPABILITY_LIE                       = 1;
  FIRMWARE_DOWNGRADE                   = 2;
  DRIFT_UNAPPROVED                     = 3;
  IOMMU_REQUIRED_ABSENT                = 4;
  REMOVABLE_DENIED                     = 5;
  HEALTH_REPEATED_FAILURE              = 6;
  POLICY_BUNDLE_DENY                   = 7;
  OPERATOR_INITIATED                   = 8;
}

enum HardwareGraphErrorCode {
  HARDWARE_GRAPH_ERROR_CODE_UNSPECIFIED = 0;
  GRAPH_NOT_INITIALIZED                 = 1;
  GRAPH_SIGNATURE_INVALID               = 2;
  DEVICE_UNCLASSIFIABLE                 = 3;
  DRIVER_BIND_REJECTED                  = 4;
  REMOVABLE_DEVICE_DENIED               = 5;
  FIRMWARE_DOWNGRADE_DETECTED           = 6;
  IOMMU_REQUIRED_ABSENT                 = 7;
  DRIFT_WITHOUT_APPROVAL                = 8;
  HOST_CAPABILITY_LIE                   = 9;
  AI_FORBIDDEN_OPERATION                = 10;
  RECOVERY_REQUIRED                     = 11;
  POLICY_REFUSED                        = 12;
  DEVICE_NOT_FOUND                      = 13;
}

// ============================================================================
// Core types
// ============================================================================

message CapabilityProbe {
  map<string, string> fields = 1;
}

message Device {
  string device_id                       = 1;
  DeviceClass class                      = 2;
  string model_string                    = 3;
  string vendor_id                       = 4;
  string device_id_pci_usb               = 5;
  string serial_number                   = 6;
  BusKind bus_kind                       = 7;
  string bus_path                        = 8;
  string driver_id                       = 9;
  DriverProvenance driver_provenance     = 10;
  string driver_version                  = 11;
  string firmware_version                = 12;
  bool firmware_trusted                  = 13;
  DeviceTrustClass trust_class           = 14;
  DeviceLifecycleState lifecycle_state   = 15;
  RemovableDevicePolicy removable_policy = 16;
  bool removable                         = 17;
  bool iommu_group_present               = 18;
  uint32 iommu_group_id                  = 19;
  CapabilityProbe capabilities_advertised = 20;
  CapabilityProbe capabilities_observed   = 21;
  google.protobuf.Timestamp first_seen_at = 22;
  google.protobuf.Timestamp last_seen_at  = 23;
}

message HardwareGraphMeta {
  string host_id                  = 1;
  bool recovery_mode              = 2;
  bool iommu_enforced_globally    = 3;
  string boot_evidence_anchor     = 4;
  uint32 device_count             = 5;
}

message HardwareGraph {
  string snapshot_id                  = 1;
  google.protobuf.Timestamp built_at  = 2;
  string previous_snapshot_id         = 3;
  repeated Device devices             = 4;
  HardwareGraphMeta meta              = 5;
  bytes ed25519_signature             = 6;
}

message DeviceDiffEntry {
  enum Kind {
    KIND_UNSPECIFIED = 0;
    ADDED            = 1;
    REMOVED          = 2;
    MUTATED          = 3;
  }
  Kind kind                       = 1;
  Device before                   = 2; // empty for ADDED
  Device after                    = 3; // empty for REMOVED
  repeated string mutated_fields  = 4; // populated for MUTATED
}

// ============================================================================
// RPC request/response
// ============================================================================

message GetActiveSnapshotRequest {}
message GetActiveSnapshotResponse {
  HardwareGraph graph = 1;
}

message GetSnapshotRequest {
  string snapshot_id = 1;
}
message GetSnapshotResponse {
  HardwareGraph graph = 1;
}

message ListDevicesRequest {
  repeated DeviceClass class_filter             = 1;
  repeated DeviceLifecycleState state_filter    = 2;
}
message ListDevicesResponse {
  repeated Device devices = 1;
}

message GetDeviceRequest {
  string device_id = 1;
}
message GetDeviceResponse {
  Device device = 1;
}

message ProbeDeviceCapabilitiesRequest {
  string device_id = 1;
}
message ProbeDeviceCapabilitiesResponse {
  CapabilityProbe advertised = 1;
  CapabilityProbe observed   = 2;
  bool mismatch_detected     = 3;
}

message ApproveRemovableDeviceRequest {
  string device_id           = 1;
  string requesting_subject  = 2;
  string policy_class        = 3;  // "ALLOW_AUTO_THIS_BOOT" | "ALLOW_AUTO_FOREVER" | "ALLOW_AUTO_FOR_GROUP_<g>"
  google.protobuf.Duration ttl = 4;
}
message ApproveRemovableDeviceResponse {
  oneof result {
    RemovableDeviceGrant grant = 1;
    HardwareGraphError error   = 2;
  }
}

message RevokeRemovableDeviceApprovalRequest {
  string grant_id = 1;
  string reason   = 2;
}
message RevokeRemovableDeviceApprovalResponse {
  bool revoked = 1;
}

message QuarantineDeviceRequest {
  string device_id                   = 1;
  DeviceQuarantineReason reason      = 2;
  string operator_subject            = 3;
}
message QuarantineDeviceResponse {
  bool quarantined = 1;
}

message ResumeDeviceRequest {
  string device_id        = 1;
  string operator_subject = 2;
  string justification    = 3;
}
message ResumeDeviceResponse {
  bool resumed = 1;
}

message RebindDriverRequest {
  string device_id                  = 1;
  string target_driver_id           = 2;
  DriverProvenance target_provenance = 3;
  string operator_subject           = 4;
}
message RebindDriverResponse {
  bool rebound = 1;
}

message AcceptDriftRequest {
  string current_snapshot_id = 1;
  repeated string accepted_device_ids = 2;
  string operator_subject    = 3;
  string justification       = 4;
}
message AcceptDriftResponse {
  bool accepted = 1;
}

message RetireDeviceRequest {
  string device_id        = 1;
  string operator_subject = 2;
}
message RetireDeviceResponse {
  bool retired = 1;
}

message DiffSnapshotsRequest {
  string from_snapshot_id = 1;
  string to_snapshot_id   = 2;
}
message DiffSnapshotsResponse {
  repeated DeviceDiffEntry entries = 1;
}

message RemovableDeviceGrant {
  string grant_id              = 1;  // grnt_<ulid>
  string device_id             = 2;
  string approving_subject     = 3;
  string policy_class          = 4;
  google.protobuf.Timestamp issued_at  = 5;
  google.protobuf.Timestamp expires_at = 6;
  bytes ed25519_signature      = 7;
}

message HardwareGraphError {
  HardwareGraphErrorCode code = 1;
  string message              = 2;
}
```
