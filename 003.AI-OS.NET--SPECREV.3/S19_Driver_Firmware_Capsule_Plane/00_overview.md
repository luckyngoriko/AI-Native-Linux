# S19 - Driver and Firmware Capsule Plane

| Field     | Value                                                                                                                                                                                                                                                                      |
| --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-28; awaiting implementation evidence)                                                                                                                                                                                                          |
| Phase tag | S19                                                                                                                                                                                                                                                                        |
| Layer     | Cross-cutting: L1, L2, L4, L6, L8, L9, L10                                                                                                                                                                                                                                 |
| Consumes  | S8.3 Hardware Graph, S8.5 Firmware Trust, S9.1 Recovery Boundary, S16.1 Security Profile Matrix, S16.4 Measured Boot, S18 Kernel Personality and Portability Plane, S17.1 AppCapsule, S12.2 Package Model, S2.3 Policy Kernel, S3.2 Sandbox Composition, S3.1 Evidence Log |
| Produces  | `DriverCapsule`, `DriverCandidate`, `DriverCompatibilityMatrix`, `DriverSolver`, driver lab, signed module lifecycle, driver rollback evidence                                                                                                                             |

## 1. Responsibility

S19 defines how AIOS handles hardware drivers, kernel modules, firmware-coupled
drivers, vendor binary drivers, DKMS/akmods-style rebuilds, and driver fallback
paths.

Drivers are not normal applications. A faulty or malicious driver can crash the
kernel, bypass sandboxing, expose memory, break GPU/video, corrupt storage, or
invalidate a high-security profile. AIOS therefore treats drivers as privileged
capsules with stricter rules than app capsules.

Invariant links: INV-002, INV-004, INV-005, INV-008, INV-012, INV-013,
INV-014, INV-017, INV-024, INV-028.

This is an intentional single-file contract overview per DEC-R3-008; missing
schemas (`FirmwareMinimum`, `CanaryBootEvidence`) are added in place rather than
decomposed into numbered sub-specs.

## 2. Product principle

AIOS must make drivers easy for the operator, but never casual for the system.

```text
hardware need
  -> HardwareGraph identity
  -> DriverSolver candidates
  -> compatibility and security scoring
  -> driver lab / isolated build
  -> signed DriverCapsule
  -> canary boot or controlled bind
  -> evidence
  -> promote, degrade, fallback, or block
```

The default answer is not "install random vendor script as root." The default
answer is: identify the device, choose the safest compatible driver path, prove
the path, and keep rollback.

## 3. Reference patterns

| Pattern                                                                                          | S19 use                                                                                   |
| ------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------- |
| [Linux kernel driver model](https://docs.kernel.org/driver-api/driver-model/)                    | Device/driver binding model and probe discipline.                                         |
| [Linux kernel module signing](https://kernel.org/doc/html/next/admin-guide/module-signing.html)  | Signed module verification and load-time enforcement.                                     |
| [Linux tainted kernels](https://www.kernel.org/doc/html/latest/admin-guide/tainted-kernels.html) | Evidence when proprietary, forced, or unsafe modules affect kernel trust.                 |
| [kmod](https://github.com/kmod-project/kmod)                                                     | Linux module dependency, alias, load, and blacklist tooling.                              |
| [DKMS](https://github.com/dkms-project/dkms)                                                     | Rebuild model for out-of-tree modules; AIOS uses the concept but not blind host mutation. |
| [LVFS / fwupd](https://lvfs.readthedocs.io/en/latest/intro.html)                                 | Firmware discovery, metadata, secure update flow, and supported-device model.             |
| [LVFS security model](https://lvfs.readthedocs.io/en/latest/security.html)                       | Vendor scoping, metadata security, and UEFI update constraints.                           |
| [systemd hwdb](https://www.freedesktop.org/software/systemd/man/latest/hwdb.html)                | Hardware ID metadata normalization for device matching.                                   |

## 4. Driver classes

```text
DriverClass =
  IN_TREE_KERNEL_DRIVER
| DISTRO_SIGNED_MODULE
| AIOS_VERIFIED_MODULE
| VENDOR_BINARY_MODULE
| SOURCE_BUILT_MODULE
| DKMS_STYLE_MODULE
| FIRMWARE_ONLY_BUNDLE
| USERSPACE_DRIVER
| VM_PASSTHROUGH_DRIVER
| BLOCKED_DRIVER
```

Preference order:

```text
in-tree signed driver
  > AIOS verified signed module
  > distro signed module
  > source-built signed module
  > vendor binary signed module
  > userspace driver
  > VM passthrough fallback
  > blocked
```

The solver may override this order only with an explicit reason, such as a GPU
driver where the vendor path is required for CUDA, NVENC, professional OpenGL,
or anti-cheat compatibility.

## 5. Driver capsule

A `DriverCapsule` is the driver equivalent of an app capsule, but it is tied to
kernel ABI, hardware identity, firmware state, security profile, and boot
rollback.

```yaml
driver_capsule:
  capsule_id: "drvcap_<ULID>"
  driver_id: "driver:nvidia:example"
  class: VENDOR_BINARY_MODULE
  source:
    origin: "vendor|distro|aios|source|local"
    uri: "mirror://..."
    signature_chain: []
    license: "proprietary|gpl|dual|unknown"
    sbom_ref: "optional"
    provenance_ref: "optional"
  hardware_match:
    device_classes: [GPU_DISCRETE]
    modaliases: ["pci:v000010DEd..."]
    vendor_ids: ["0x10de"]
    device_ids: []
    subsystem_ids: []
    firmware_minimums: [] # list of FirmwareMinimum (see §9); empty = no firmware floor
  kernel_compatibility:
    kernel_personality: LINUX_HARDENED
    kernel_release: "example"
    kernel_config_hash: "sha256:..."
    vermagic: "example"
    compiler_abi: "example"
    required_symbols: []
    blocked_symbols: []
  module:
    ko_hash: "sha256:..."
    module_signing: AIOS_SIGNED
    load_parameters: {}
    blacklists: []
    conflicts: []
  security:
    allowed_profiles: [SECURE_DEFAULT]
    forbidden_profiles: [AIRGAP_HIGH]
    taint_expected: true
    recovery_required: true
  lifecycle:
    state: STAGED
    rollback_target: "previous_driver_capsule_id"
  evidence:
    build_receipt: "evr_..."
    test_receipt: "evr_..."
    bind_receipt: "evr_..."
```

Driver capsule filesystem layout:

```text
/aios/system/drivers/<driver_id>/
  driver-capsule.toml
  source/
  build/
  module/
  firmware/
  tests/
  logs/
  rollback/
  evidence/
```

## 6. Driver solver

The DriverSolver consumes the active `HardwareGraph` and the active
`KernelCapabilityMatrix`.

Inputs:

- device identity: vendor id, device id, subsystem id, revision, serial, bus
  path, modalias, firmware version
- kernel personality, kernel version, config hash, ABI/vermagic
- security profile and Secure Boot posture
- workload goal: generic, gaming, AI GPU, storage, network, RT, mobile, server
- available driver candidates from AIOS registry, distro repos, vendor feeds,
  local operator bundles, source builds
- known-bad list and compatibility ledger

Outputs:

```text
DriverDecision =
  USE_IN_TREE
| INSTALL_SIGNED_CAPSULE
| BUILD_SOURCE_CAPSULE
| USE_VENDOR_CAPSULE
| USE_USERSPACE_DRIVER
| ROUTE_TO_VM
| KEEP_DEGRADED
| BLOCK_WITH_REASON
```

The operator sees the decision as a risk diff:

```text
Device: NVIDIA GPU
Recommended path: vendor binary capsule
Benefit: CUDA/NVENC/gaming support
Risk: proprietary module taints kernel
Controls: signed module, canary boot, rollback, evidence
Fallback: nouveau/in-tree degraded graphics or Linux VM GPU host
```

## 7. Driver lab

Unknown or high-risk drivers enter Driver Lab before promotion.

```text
candidate driver
  -> parse metadata
  -> verify source/signature/license
  -> build in isolated environment if needed
  -> sign module with AIOS key when policy allows
  -> static checks
  -> dependency and symbol checks
  -> simulated boot where possible
  -> controlled canary boot on real hardware
  -> health probes
  -> promote or rollback
```

Simulation is useful but not sufficient. Real devices have firmware, ACPI, PCIe,
IOMMU, GPU, power, and BIOS behavior that cannot be fully proven in a VM. A
driver touching real hardware needs canary boot or controlled bind evidence.

Driver Lab checks:

| Check                 | Purpose                                                                              |
| --------------------- | ------------------------------------------------------------------------------------ |
| `metadata_parse`      | Extract driver identity, module aliases, license, kernel ABI, firmware dependencies. |
| `signature_verify`    | Reject unknown or broken signature chains.                                           |
| `known_bad_lookup`    | Block drivers with known regressions, CVEs, broken firmware, or bad kernel ranges.   |
| `build_reproducible`  | Build from source in hermetic environment when source path is used.                  |
| `module_sign`         | Sign modules for Secure Boot / strict profiles.                                      |
| `symbol_check`        | Verify required kernel symbols and ABI compatibility.                                |
| `taint_predict`       | Predict and record expected kernel taint.                                            |
| `boot_simulation`     | Boot with candidate where hardware-independent checks are possible.                  |
| `canary_boot`         | Boot once into candidate profile and return evidence.                                |
| `device_health_probe` | Confirm device binds, reports expected capabilities, and does not lie.               |

### 7.1 CanaryBootEvidence schema

The `canary_boot` check produces a `CanaryBootEvidence` record. This is the
contract that gives `DRIVER_CANARY_BOOT_PASSED` and `DRIVER_CANARY_BOOT_FAILED`
(see §12) their defined minimum fields. Promotion (§10) consumes this record;
a candidate cannot be promoted without a present, well-formed `CanaryBootEvidence`.

```yaml
canary_boot_evidence:
  candidate_driver_id: "driver:nvidia:example" # driver under canary
  boot_outcome: BOOTED_CLEAN # closed enum below
  device_health_probe_result: HEALTHY # closed enum below
  taint_observed: false # actual kernel taint observed during the canary boot
  rollback_ready: true # previous module/boot/blacklist state confirmed restorable
  evidence_receipt_id: "evr_..." # receipt for this canary boot in the S3.1 log
```

Closed `boot_outcome` enum:

```text
CanaryBootOutcome =
  BOOTED_CLEAN        # candidate booted and reached the canary profile
| BOOTED_DEGRADED     # booted but with reduced capability or warnings
| BOOT_FAILED         # candidate did not reach the canary profile
| BOOT_HUNG           # boot stalled; recovered via rollback/watchdog
```

Closed `device_health_probe_result` enum:

```text
DeviceHealthProbeResult =
  HEALTHY             # device bound and reported expected capabilities truthfully
| DEGRADED            # device bound with reduced or partial capability
| MISREPORTING        # device reported capabilities it does not have ("lies")
| ABSENT              # device did not bind / did not appear
| NOT_PROBED          # probe could not run this canary cycle
```

Unknown values for `boot_outcome` or `device_health_probe_result` are rejected by
Driver Lab: a canary cycle that cannot record a known enum value fails closed and
emits `DRIVER_CANARY_BOOT_FAILED`. Promotion requires
`boot_outcome ∈ {BOOTED_CLEAN, BOOTED_DEGRADED}`,
`device_health_probe_result ∈ {HEALTHY, DEGRADED}`, `rollback_ready == true`, and
`taint_observed` consistent with the capsule's declared `taint_expected` (§5);
any other combination yields `DRIVER_CANARY_BOOT_FAILED` and blocks promotion.

## 8. Security profile gates

| Profile          | Driver rule                                                                       |
| ---------------- | --------------------------------------------------------------------------------- |
| `DEV_RELAXED`    | Source-built or vendor modules allowed with warning and rollback.                 |
| `SECURE_DEFAULT` | Signed drivers preferred; unsigned blocked unless operator exception.             |
| `STIG_ALIGNED`   | In-tree, AIOS-verified, or distro-signed only unless recovery-approved exception. |
| `AIRGAP_HIGH`    | Signed local mirror only; no live vendor downloads; no unsigned modules.          |

Hard denies:

- no unsigned kernel module under `STIG_ALIGNED` or `AIRGAP_HIGH`
- no AI approval for driver install, bind, blacklist removal, or firmware update
- no vendor install script may run as root outside Driver Lab
- no driver may bypass the HardwareGraph lifecycle
- no driver may be promoted without rollback path
- no driver conflict may silently unload another active driver

Driver signing and Secure Boot posture are governed by S16.4 Measured Boot, not
re-defined here: S19 reads the active Secure Boot / lockdown posture from S16.4 and
requires AIOS module signing (`module_sign`, §7) before any module load under
`STIG_ALIGNED`/`AIRGAP_HIGH`. Per INV-028, an AI subject can neither author nor alter
the boot-integrity expectations S16.4 owns; S19 only consumes that posture and may at
most propose a typed remediation for a Policy Kernel decision.

## 9. Firmware coupling

Some driver problems are firmware problems. S19 must not duplicate S8.5; it
binds to it.

Examples:

- GPU driver requires a minimum GPU firmware version.
- Wi-Fi driver requires signed regulatory database or firmware blob.
- NVMe driver exposes features only after controller firmware update.
- Thunderbolt/USB4 driver is blocked if IOMMU protection is absent.

### 9.1 FirmwareMinimum schema

The `firmware_minimums` field on a `DriverCapsule.hardware_match` (see §5) is a
list of `FirmwareMinimum` entries. Each entry declares one firmware floor the
driver requires and how it is compared. The empty list means the driver imposes
no firmware floor.

```yaml
firmware_minimum:
  component: "gpu_vbios" # which firmware component this floor applies to;
  # the component string MUST resolve to a firmware component reported by the
  # S8.5 firmware state for the matched device (no free-form components)
  min_version: "94.02.42.00.01" # vendor-format minimum acceptable version string
  comparator: SEMVER_GTE # how min_version is compared to the observed S8.5 version
  source_of_truth: S8_5_FIRMWARE_STATE # observed version always read from S8.5,
  # never self-reported by the driver capsule
  on_unmet: BLOCK_WITH_REASON # action when the floor is not satisfied
```

Closed `comparator` enum:

```text
FirmwareComparator =
  SEMVER_GTE          # observed firmware version >= min_version, semver ordering
| VENDOR_ORDINAL_GTE  # observed >= min_version, vendor-declared monotonic ordinal
| EXACT_MATCH         # observed == min_version (locked firmware build)
| MIN_BUILD_DATE_GTE  # observed firmware build date >= min_version (date-encoded)
```

Unknown values are rejected by the DriverSolver: an unrecognized `comparator`,
an unresolvable `component`, or a missing `min_version` makes the candidate
non-usable (treated as `firmware minimums match = false`), not silently skipped.

Closed `on_unmet` enum (a subset of the §6 decision space):

```text
FirmwareUnmetAction =
  BLOCK_WITH_REASON   # driver not usable until firmware floor is met
| KEEP_DEGRADED       # remain on the safe/in-tree fallback driver
| ROUTE_TO_VM         # route the workload to a VM instead of host bind
```

Unknown values are rejected by the DriverSolver.

### 9.2 Comparison against S8.5 firmware state

S19 owns the comparison only; S8.5 owns the firmware truth. The DriverSolver reads
the observed firmware version and trust verdict from the active S8.5 firmware state
for the matched device and evaluates each `FirmwareMinimum` against it. A driver
candidate is **usable only if firmware trust passes (per S8.5) AND every
`FirmwareMinimum` is satisfied under its declared comparator AND downgrade/tamper
checks pass (per S8.5)**. S19 never re-derives, caches, or self-asserts a firmware
version: the comparison input is always the live S8.5 state.

Rule:

```text
driver candidate usable
  only if firmware trust passes              (S8.5 verdict)
  and every FirmwareMinimum matches          (S19 comparison over S8.5 observed version)
  and downgrade/tamper checks pass           (S8.5)
```

A failed comparison emits `DRIVER_FIRMWARE_REQUIREMENT_FAILED` (see §12) and the
candidate is not bound.

Firmware-only updates remain owned by S8.5. Driver capsules may carry firmware
references, but firmware apply still follows S8.5 approval, staging, rollback,
and evidence rules.

## 10. Rollback and fallback

Every driver mutation has a fallback plan:

| Mutation                | Required rollback                                                      |
| ----------------------- | ---------------------------------------------------------------------- |
| Kernel module install   | Previous module set retained and bootable.                             |
| Driver blacklist change | Previous blacklist restored on failed boot.                            |
| Boot parameter change   | Previous boot entry retained.                                          |
| Firmware-coupled driver | Firmware rollback path or block promotion.                             |
| GPU driver change       | Text console/recovery graphics path verified.                          |
| Storage driver change   | Offline recovery boot must see root filesystem before promotion.       |
| Network driver change   | Local recovery path required; remote-only host cannot promote blindly. |

Fallback outcomes:

```text
PROMOTE
ROLL_BACK
KEEP_DEGRADED
ROUTE_TO_VM
BLOCK_WITH_REASON
```

## 11. Operator UX

The operator should see a clear Driver Passport, not logs full of kernel jargon.

Minimum UI fields:

- device name and hardware ids
- current driver and candidate driver
- source and trust chain
- kernel compatibility
- firmware requirements
- expected benefit
- risk and kernel taint
- affected apps/capsules
- rollback plan
- canary result
- evidence receipt

One-click operator actions:

```text
Use safe driver
Use performance driver
Keep degraded
Test candidate
Rollback driver
Block this driver
Route workload to VM
```

Each action still maps to a typed policy decision. The UI does not become
authority.

## 12. Evidence records

S19 adds these record types:

```text
DRIVER_CANDIDATE_DISCOVERED
DRIVER_SOLVER_DECISION
DRIVER_CAPSULE_STAGED
DRIVER_CAPSULE_BUILT
DRIVER_CAPSULE_SIGNED
DRIVER_LAB_CHECK_RESULT
DRIVER_CANARY_BOOT_STARTED
DRIVER_CANARY_BOOT_PASSED
DRIVER_CANARY_BOOT_FAILED
DRIVER_BOUND
DRIVER_BIND_BLOCKED
DRIVER_ROLLED_BACK
DRIVER_FALLBACK_SELECTED
DRIVER_KERNEL_TAINT_RECORDED
DRIVER_KNOWN_BAD_MATCH
DRIVER_FIRMWARE_REQUIREMENT_FAILED
```

Minimum fields for `DRIVER_SOLVER_DECISION`:

```text
device_id
hardware_graph_snapshot_id
kernel_personality
kernel_release
driver_candidates
selected_driver_id
decision
expected_benefit
risk_summary
security_profile
rollback_plan_id
evidence_receipt_id
```

Minimum fields for `DRIVER_CANARY_BOOT_PASSED` and `DRIVER_CANARY_BOOT_FAILED`
are the `CanaryBootEvidence` schema (§7.1): `candidate_driver_id`, `boot_outcome`,
`device_health_probe_result`, `taint_observed`, `rollback_ready`,
`evidence_receipt_id`.

Minimum fields for `DRIVER_FIRMWARE_REQUIREMENT_FAILED`:

```text
device_id
candidate_driver_id
component
min_version
comparator
observed_firmware_version   # from S8.5 firmware state (§9.2)
firmware_trust_verdict      # from S8.5
on_unmet
evidence_receipt_id
```

## 13. Non-goals

- Do not promise every vendor driver works.
- Do not run vendor `.run`, shell, or installer scripts directly on the host.
- Do not hide proprietary-driver taint or Secure Boot implications.
- Do not make gaming/GPU driver needs weaken hardened profiles silently.
- Do not treat firmware blobs as normal files outside S8.5 trust rules.
- Do not require a custom kernel when an in-tree driver works well.

## 14. Acceptance criteria

S19 is `REAL` only when:

1. Driver capsules parse and reject unknown driver classes.
2. HardwareGraph device ids can be matched to candidate drivers.
3. At least one in-tree driver path and one out-of-tree staged path are modeled.
4. DriverSolver emits a typed decision with benefit, risk, and fallback.
5. Unsigned modules are blocked under strict profiles.
6. Driver Lab can stage/build/test without mutating the active host.
7. Canary boot evidence controls promotion.
8. Rollback restores previous module/boot/blacklist state.
9. Kernel taint is recorded when expected or observed.
10. AI subjects cannot approve driver install, bind, rollback, or exception.

## 15. See also

- [S8.3 Hardware Graph](../../002.AI-OS.NET--SPECREV.2/L8_Network_Hardware_Devices/01_hardware_graph.md)
- [S8.5 Firmware Trust](../../002.AI-OS.NET--SPECREV.2/L8_Network_Hardware_Devices/04_firmware_trust.md)
- [S16 Security Hardening and Compliance](../S16_Security_Hardening_Compliance/00_overview.md)
- [S16.4 Measured Boot and Runtime Integrity](../S16_Security_Hardening_Compliance/04_measured_boot_runtime_integrity.md)
- [Rev.3 Constitutional Invariants (INV-028)](../04_invariants.md)
- [Rev.3 Design Decisions (DEC-R3-008)](../02_design_decisions.md)
- [S17 App Capsule Runtime](../S17_App_Capsule_Runtime/00_overview.md)
- [S18 Kernel Personality and Portability Plane](../S18_Kernel_Personality_Portability/00_overview.md)
