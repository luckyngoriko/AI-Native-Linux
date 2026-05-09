# Dedicated Kernel Pipeline (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists; structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| Phase tag      | S9.3                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Layer          | L1 Kernel, Bootstrap, Recovery                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Schema package | `aios.kernel.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| Consumes       | **Imports vocabulary from**: S10.1 Capability Runtime (`ActionLifecycleState`, `ActionDispatchKind = ISOLATED_SANDBOX`, `AdapterIOMode = TYPED_PARAMETERS_ONLY`, `ExecutionFailureReason = EXECUTION_VERIFICATION_FAILED` — closed-enum shapes; type-level), S3.2 Sandbox Composition (`SandboxProfile`, runtime safety floor, AI-floor — type-level profile schema), S11.1 Repository Model (`PackageKind = KERNEL_CANDIDATE`, `RepositoryKind = AIOS_RECOVERY_REPO`, `UpdateChannel = RECOVERY_CRITICAL` — type-level package vocabulary), S8.2 GPU Resource Model (hardware graph snapshot id format — type-level), L8 HDM (`HardwareGraphSnapshotId` shape — type-level), S3.1 Evidence Log (`RecordType` + retention classes + hash chain shape — type-level), S5.2 Vault Broker (`KEY_SIGN`, `KEY_VERIFY` capability schema — type-level), S5.1 Identity Model (`_system:service:kernel-builder` subject scope — type-level identifier shape). **Requires for correctness (degraded subset only)**: S9.1 Recovery Boundary (`RecoveryStage`, `RecoveryMutableScope = DEDICATED_KERNEL_PROMOTION` and `L1_BOOT_PARAMETERS`, A/B kernel slot semantics, `RecoveryEntryReason = EVIDENCE_LOG_TAMPER_DETECTED` — peer L1 dependency; the build-pipeline writes into recovery-mode-gated mutable scopes). **Peer (intra-L0)**: INV-001, INV-004, INV-005, INV-013, INV-014, INV-018. |
| Produces       | typed `KernelBuilder.Build` deterministic function signature; closed `ConvergenceState`, `GateName`, `GateResult`, `AdditionFilterResult`, `SecurityTarget`, `KernelImageState`, `KernelMaintenanceResult` enums; the six machine-checkable acceptance gates with measures and thresholds; the 3-AND addition filter; the closed accepted/rejected feature tables; the A/B promotion FSM with allowed-transition table; the kernel-image drift detection contract; the maintenance/refresh discipline; thirteen evidence record types queued for S3.1 Wave 7; bounded-cardinality telemetry contract; three worked examples covering bootstrap, refresh failure, and evil-maid drift                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |

## §1 Purpose

The dedicated kernel pipeline is the mechanism by which AIOS produces, validates, promotes, observes, and refreshes the kernel image that runs the host. The generic fallback kernel (the distribution-supplied vmlinuz that any vanilla Linux installation would carry) is always present at slot B and provides the recovery floor mandated by INV-001 and S9.1; the **dedicated** kernel is the AIOS-tailored image that — when validated — runs at slot A and serves the normal-mode workload. Everything in this spec exists to answer one operational question rigorously: under what mechanical conditions is a kernel image worthy of running in slot A, and how does the system detect when that condition stops holding?

The mechanism in this spec is structural, not stylistic. It has eight tightly coupled elements (§4 through §11). Each element is necessary; if any single element is removed the mechanism becomes stupid regardless of the time available, the team size, or the hardware budget. The criterion is correctness of structure, not effort: "we will harden this in a later phase" is not a property of a smart pipeline — a smart pipeline either has the eight elements or it does not.

The eight elements are:

1. The build is a typed, deterministic function (§4) — identical inputs produce a bit-identical image.
2. Convergence is a fixed-point on the input/output hash, with monotonic gate scores (§5) — there is a definition of "done iterating".
3. Six machine-checkable acceptance gates fully bind promotion (§6) — STABILITY, SECURITY, HARDWARE_FIT, PERFORMANCE, REPRODUCIBILITY, RECOVERY_REHEARSAL. All-of, no human override.
4. Kernel additions pass a 3-AND filter (§7) — every accepted feature must enforce a cited L0 invariant, exist upstream, and have a runtime measurement.
5. The build itself is an AIOS typed action under the L3 Capability Runtime, governed by the same rules every other action is governed by (§8).
6. Promotion is a closed A/B FSM with recovery-mode-gated transitions (§9).
7. The running kernel image is itself an evidence subject — its hash is observed at every boot, and drift triggers immediate recovery (§10).
8. Maintenance is structural, not heroic — the pipeline runs on schedule and reuses the same gates (§11).

This contract closes a recursion that would otherwise stay implicit: AIOS builds, hardens, promotes, observes, and refreshes its own kernel using AIOS's own action governance and evidence machinery. The build plane is the runtime plane. There is no separate, less-governed "build system" living outside the constitution.

## §2 Scope

This spec **defines**:

1. The deterministic `KernelBuilder.Build` function signature, the closed structure of its inputs, and the bit-identical-output contract.
2. The closed `ConvergenceState` enum and the formal fixed-point criterion (§5).
3. The closed `GateName` and `GateResult` enums (§6) and the per-gate threshold table.
4. The 3-AND `AdditionFilterResult` and `SecurityTarget` enums (§7) and the closed accepted/rejected feature tables.
5. The mapping of the build to a typed `kernel.build` action under S10.1 (§8) — dispatch kind, adapter IO mode, signing subject, evidence linkage, failure semantics.
6. The closed `KernelImageState` FSM and the allowed-transition table (§9) — including recovery-mode gating for promotion to slot A.
7. The kernel-image drift detection contract (§10) — every-boot hash observation, evidence emission on mismatch, automatic recovery entry.
8. The closed `KernelMaintenanceResult` enum and the scheduled `kernel.refresh` action contract (§11).
9. The bounded-cardinality telemetry contract (§14).
10. Thirteen evidence record types queued for S3.1 (§15).
11. Three worked examples covering bootstrap, periodic refresh failure, and evil-maid kernel swap (§16).

This spec **does not** define:

- The wire shape of `HardwareGraphSnapshot` or the Hardware Discovery & Mapping (HDM) service. Owned by L8 HDM (deferred sub-spec). This contract consumes the snapshot id only.
- The first-boot installer flow. Owned by S9.2 (`02_first_boot_flow.md`).
- The recovery boundary mechanics, recovery shell, or operator authentication. Owned by S9.1 (`01_recovery_boundary.md`).
- Hardware attestation in recovery (TPM remote attestation). Deferred (§17).
- Multi-architecture cross-compilation (x86_64 vs aarch64 cross-build). Deferred (§17).
- Per-group kernel variants — there is exactly one A slot per host. Rejected (§17).
- The vault broker's signing flow. Owned by S5.2 (`KEY_SIGN`, `KEY_VERIFY` consumed abstractly).
- The L3 SGR scheduler that fires `kernel.refresh` actions on cadence. Owned by L3 SGR.

This spec is the **contract surface** that S9.1 references when it says "A/B kernel fallback contract (full pipeline lives in S9.3)" and that S11.1 references when it says "`KERNEL_CANDIDATE` installs feed into S9.3". Both consumers can now resolve those references mechanically.

## §3 Position in the system

The dedicated kernel pipeline sits between the L10 distribution layer (which delivers `KERNEL_CANDIDATE` packages from the `AIOS_RECOVERY_REPO` per S11.1) and the L1 boot path (which selects which kernel image GRUB loads per S9.1 §4.1). The pipeline is invoked by the L3 Capability Runtime as a typed `kernel.build` action; the pipeline produces a `KernelImage`; the image flows through a six-gate validation; if all six gates pass, the image becomes a candidate for A-slot promotion under recovery mode.

```text
                        ┌─────────────────────────────────────────────────────────┐
                        │ L10 AIOS_RECOVERY_REPO (S11.1)                          │
                        │   PackageKind = KERNEL_CANDIDATE                        │
                        │   UpdateChannel = RECOVERY_CRITICAL                     │
                        │   Recovery-only install (S9.1 RecoveryMutableScope =    │
                        │     DEDICATED_KERNEL_PROMOTION)                         │
                        └─────────────────────────┬───────────────────────────────┘
                                                  │  upstream pinned tarball
                                                  v
   ┌────────────────────────────────────────────────────────────────────────┐
   │ S9.3 Dedicated Kernel Pipeline (this spec)                              │
   │                                                                          │
   │   ┌───────────────────────────────────┐                                 │
   │   │ §4 KernelBuilder.Build            │  typed deterministic function   │
   │   │   in:  HardwareGraphSnapshotId    │                                 │
   │   │        SecurityTarget             │                                 │
   │   │        InvariantBundleId          │                                 │
   │   │        UpstreamKernelVersion      │                                 │
   │   │        ModuleSet                  │                                 │
   │   │        FirmwareBlobSet            │                                 │
   │   │        SeccompProfile             │                                 │
   │   │        LsmConfig                  │                                 │
   │   │   out: KernelImage (bit-identical │                                 │
   │   │        BLAKE3 across rebuilds)    │                                 │
   │   └─────────────────┬─────────────────┘                                 │
   │                     │                                                   │
   │                     v                                                   │
   │   ┌───────────────────────────────────┐                                 │
   │   │ §5 Convergence (fixed-point)      │  ITERATING ─▶ CONVERGED          │
   │   │     monotonic gate scores         │            ─▶ DIVERGED (fix)     │
   │   │                                   │            ─▶ ABANDONED          │
   │   └─────────────────┬─────────────────┘                                 │
   │                     │                                                   │
   │                     v                                                   │
   │   ┌───────────────────────────────────┐                                 │
   │   │ §6 Six gates (all-of)             │                                 │
   │   │     STABILITY                     │                                 │
   │   │     SECURITY                      │                                 │
   │   │     HARDWARE_FIT                  │                                 │
   │   │     PERFORMANCE                   │                                 │
   │   │     REPRODUCIBILITY               │                                 │
   │   │     RECOVERY_REHEARSAL            │                                 │
   │   │   any FAIL ─▶ pipeline halts      │                                 │
   │   └─────────────────┬─────────────────┘                                 │
   │                     │ all 6 PASSED                                      │
   │                     v                                                   │
   │   ┌───────────────────────────────────┐                                 │
   │   │ §9 KernelImageState FSM           │                                 │
   │   │     BUILT ─▶ GATING ─▶ GATE_PASSED                                  │
   │   │     ─▶ A_PROMOTED (recovery-mode) │                                 │
   │   │     ─▶ B_DEMOTED_TO_A (after N    │                                 │
   │   │        consecutive boots)         │                                 │
   │   │     ─▶ ROLLBACK on N=2 fails      │                                 │
   │   │     ─▶ RETIRED (forensic archive) │                                 │
   │   └─────────────────┬─────────────────┘                                 │
   └─────────────────────┼─────────────────────────────────────────────────┘
                         │
                         v
   ┌────────────────────────────────────────────────────────────────────────┐
   │ L1 boot path (S9.1)                                                     │
   │                                                                          │
   │   GRUB entry 0  → /boot/vmlinuz-aios     (slot A — dedicated)           │
   │   GRUB entry 1  → /boot/vmlinuz-generic  (slot B — generic fallback)    │
   │   GRUB entry 2  → recovery (dedicated)                                  │
   │   GRUB entry 3  → recovery (generic fallback)                           │
   │                                                                          │
   │   §10 Every boot:                                                        │
   │     observed_hash = BLAKE3(running_bzImage)                              │
   │     expected_hash = vault.get("current_a_kernel_image_hash")            │
   │     if observed != expected:                                            │
   │       emit KERNEL_IMAGE_DRIFT_DETECTED FOREVER                          │
   │       drop into recovery (S9.1 entry reason: KERNEL_IMAGE_TAMPER_DETECTED)│
   └────────────────────────────────────────────────────────────────────────┘
```

The recursion is constructive: AIOS builds AIOS using AIOS, and AIOS observes the running AIOS kernel using AIOS. The same governance plane (typed actions, policy decisions, sandbox profiles, evidence) is the build plane. There is no second, weaker administrative plane.

## §4 Element 1 — Build is a typed deterministic function

### §4.1 Signature

The pipeline exposes exactly one production-grade entry point. It is a typed RPC under `aios.kernel.v1alpha1`:

```proto
syntax = "proto3";
package aios.kernel.v1alpha1;

service KernelBuilder {
  // Build a kernel image from the closed input set.
  // Determinism contract: identical inputs → bit-identical KernelImage.
  rpc Build(BuildRequest) returns (BuildResponse);

  // Re-run a previously executed build attempt by id (for the
  // REPRODUCIBILITY gate; see §6.5).
  rpc Rebuild(RebuildRequest) returns (BuildResponse);

  // Read-only diagnostic — explain how each Kconfig option was decided
  // across the typed inputs.
  rpc ExplainBuild(ExplainBuildRequest) returns (ExplainBuildResponse);
}

message BuildRequest {
  // L8 HDM hardware graph snapshot id; immutable, content-addressed.
  // Per S8.2 / future L8 HDM. Must resolve to a graph the host can prove
  // it took (HDM evidence chain).
  string hardware_graph_snapshot_id = 1;

  // Closed enum (§7.4). Selects the hardening posture.
  SecurityTarget security_target = 2;

  // L0 invariant bundle id (per L0 §4) — the build is bound to one
  // invariant set; rebuilds against a different bundle are different
  // builds and must produce different images.
  string aios_invariant_bundle_id = 3;       // "invbundle_<hex_lower(BLAKE3(...))[:32]>"

  // Pinned upstream kernel tag, e.g. "linux-6.6.42". Must be a real
  // upstream stable tag; the fetch step (§8.4) verifies the tarball's
  // content hash against a signed upstream-mirror catalog. No fork.
  string upstream_kernel_version = 4;

  // The exact module set AIOS uses on this host. Closed list (§4.3).
  // Modules outside this set are stripped at build time.
  KernelModuleSet module_set = 5;

  // Signed firmware blobs required by the hardware graph. Each blob
  // is content-addressed and Ed25519-signed by its vendor.
  FirmwareBlobSet firmware_blob_set = 6;

  // Baseline AIOS seccomp profile (the runtime-safety-floor input
  // to S3.2 sandbox composition is consumed at runtime; this is the
  // *kernel default* set).
  SeccompProfile default_seccomp = 7;

  // SELinux / AppArmor / Landlock posture.
  LsmConfig lsm_config = 8;
}

message BuildResponse {
  string kernel_build_attempt_id = 1;        // "kbld_<ulid26>"
  string kernel_image_id = 2;                // "kimg_<full_blake3_64hex>"
  string kernel_image_blake3 = 3;            // hex_lower(BLAKE3(bzImage)) — full 64 hex
  ConvergenceState convergence_state = 4;
  google.protobuf.Timestamp built_at = 5;
  string pipeline_definition_id = 6;         // "kpdef_<hex_lower(BLAKE3(...))[:32]>"
  string pipeline_definition_signature = 7;  // Ed25519 over canonical pipeline-definition bytes
}

message KernelModuleSet {
  // The closed list of modules built into the image OR allowed as
  // signed loadable modules. Modules outside the list are unconditionally
  // stripped (§4.3).
  repeated string allowed_modules = 1;       // canonical module names
  repeated string built_in = 2;              // subset compiled into vmlinuz
}

message FirmwareBlobSet {
  repeated FirmwareBlob blobs = 1;
}

message FirmwareBlob {
  string blob_id = 1;                        // "fwbl_<hex_lower(BLAKE3(...))[:32]>"
  string vendor = 2;
  bytes ed25519_signature = 3;
  string content_hash = 4;                   // BLAKE3 of blob bytes (full hex)
}

message SeccompProfile {
  // Closed allow/deny syscall list. Owned by S3.2 vocabulary.
  repeated string allowed_syscalls = 1;
  repeated string denied_syscalls = 2;
}

message LsmConfig {
  bool selinux_enforcing = 1;
  bool apparmor_enforce = 2;
  bool landlock_enabled = 3;
  string lockdown_level = 4;                 // "confidentiality" required (§7.4)
}
```

### §4.2 Determinism contract

The build is **bit-identical** under identical inputs. Quantitative criterion:

> Three successive rebuilds (`Rebuild` invoked three times with the same `kernel_build_attempt_id`) must produce identical `kernel_image_blake3`. If the three hashes are not equal, the REPRODUCIBILITY gate (§6.5) fails. The pipeline cannot promote a non-deterministic image.

Determinism is achieved by:

1. **Pinned upstream tarball** — `upstream_kernel_version` resolves to an exact tarball whose BLAKE3 is verified against a signed upstream-mirror catalog before any source is touched.
2. **`KBUILD_BUILD_TIMESTAMP` and `KBUILD_BUILD_USER` are normalised** — the build environment forces deterministic timestamps and user strings. (The kernel's reproducible-build surface is upstream and well-trodden.)
3. **Toolchain pinning** — the gcc/binutils/llvm versions used by the build are part of the implicit pipeline-definition inputs (§4.4) and are content-addressed.
4. **Sorted dependency walks** — every list traversal in the build (modules, firmware blobs, included headers) is canonically sorted before consumption.
5. **No system clock dependency in the produced image** — the bzImage does not contain build-machine wall-clock metadata that varies across rebuilds.

Two consecutive rebuilds that disagree are not "flaky"; they are a contract violation of §4.2 and the pipeline halts. The pipeline never silently retries; the operator is alerted via FOREVER `KERNEL_DIVERGED_REGRESSION` evidence.

### §4.3 Module set is closed

The `KernelModuleSet.allowed_modules` field is the **closed list of modules AIOS uses**. The build process strips every other module from the final image. This is defense-in-depth: even if a hardware graph-driven enumeration mistakenly enables a module AIOS does not need, the strip pass removes it. The strip pass is part of the deterministic build (its input is the closed module set, its output is a bit-identical `bzImage`).

A request to add a module to `allowed_modules` is itself a typed action (`kernel.module_set.add`) that flows through the same governance plane. AI subjects cannot perform this action under any circumstance (INV-013). Adding a module to the closed list is a recovery-mode operation by a `HUMAN_USER` subject, recorded with FOREVER evidence.

### §4.4 Pipeline-definition is signed

The pipeline-definition itself — the protobuf-described shape of the build, the toolchain pin, the deterministic-build flags, the module-strip pass, and the gate definitions — is content-addressed and Ed25519-signed by the AIOS root key (per L0 §4 invariant bundle, S5.2 `KEY_SIGN`):

```text
pipeline_definition_id = "kpdef_" + hex_lower(BLAKE3(canonical_pipeline_definition_bytes))[:32]
```

Replacement of the pipeline-definition is a recovery-mode operation per S9.1 §3.6 `RecoveryMutableScope = L1_BOOT_PARAMETERS`. The build refuses to start if the loaded pipeline-definition's signature does not verify. A tampered pipeline-definition cannot produce a kernel that the rest of the system will treat as valid — the kernel-image hash on file in vault was sealed against the previous, signed pipeline-definition; any image produced under a different pipeline-definition will fail §10 drift detection at first boot.

### §4.5 IDs

| ID kind                    | Format                                 | Notes                                                                                                                        |
| -------------------------- | -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| Kernel build attempt       | `kbld_<ulid26>`                        | Unique per invocation of `Build`. Carries trace context.                                                                     |
| Kernel image               | `kimg_<full_blake3_64hex>`             | Full BLAKE3 of the produced `bzImage`. Not truncated. The image hash is constitutionally important; truncation is forbidden. |
| Pipeline-definition        | `kpdef_<hex_lower(BLAKE3(...))[:32]>`  | Truncated per S0.1 §8.5.                                                                                                     |
| Hardware graph snapshot id | `hwsnap_<hex_lower(BLAKE3(...))[:32]>` | Owned by L8 HDM; consumed read-only here.                                                                                    |
| Firmware blob id           | `fwbl_<hex_lower(BLAKE3(...))[:32]>`   | Truncated per S0.1 §8.5.                                                                                                     |

The kernel image hash uses **full** BLAKE3 (64 hex chars / 256 bits) on purpose. The image is a constitutional artifact — its hash gates A-slot promotion, drift detection, and forensic archive linkage. A 128-bit collision space is too small for the threat model.

## §5 Element 2 — Convergence as fixed-point

### §5.1 `ConvergenceState` enum

Closed enum, four values. The pipeline transitions between iterations of the build-and-gate loop and reports the convergence state on each transition.

```proto
enum ConvergenceState {
  CONVERGENCE_STATE_UNSPECIFIED = 0;
  ITERATING = 1;
  CONVERGED  = 2;
  DIVERGED   = 3;
  ABANDONED  = 4;
}
```

| Value       | Meaning                                                                                                                                                                                                                                                                                                                                                                   |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ITERATING` | The pipeline is in motion. The most recent iteration's input/output hash differs from the previous iteration's. Another iteration is required.                                                                                                                                                                                                                            |
| `CONVERGED` | Fixed-point reached. The current iteration's input/output hash equals the previous iteration's hash. No new iteration is needed; the produced image is a stable artifact under the closed input set.                                                                                                                                                                      |
| `DIVERGED`  | A gate score regressed compared to the previous iteration. The pipeline halts iteration until the regression is fixed (e.g., a new module addition broke STABILITY; the operator must remove or replace the module). DIVERGED is not a terminal failure of the build; it is a _halting condition_ that requires human triage. The build cannot iterate past a regression. |
| `ABANDONED` | Operator-initiated termination of the pipeline. The current iteration is discarded; FOREVER evidence records the reason.                                                                                                                                                                                                                                                  |

### §5.2 Convergence criterion

The fixed-point is defined formally on the canonical iteration tuple:

```text
let h(N) = hex_lower(BLAKE3(canonical_concat(
                itr_N.kconfig_blob,
                itr_N.module_list_canonical,
                itr_N.driver_list_canonical,
                itr_N.firmware_blob_list_canonical,
                itr_N.seccomp_profile_canonical,
                itr_N.lsm_config_canonical
           )))

converged_when h(N) == h(N+1)
```

`canonical_concat` deterministically encodes each component (canonical proto bytes for structured inputs, sorted line-by-line text for `kconfig`). The hash is over the **inputs** of two adjacent iterations; the output (`bzImage`) hash is also expected to match because of the determinism contract (§4.2), but the convergence definition operates on the input axis to make "we changed inputs that did not change the output" detectable as `ITERATING` rather than `CONVERGED`.

### §5.3 Monotonicity rule

Each iteration's gate score must be **greater than or equal to** the previous iteration's gate score. A regression fails the monotonicity rule and forces `DIVERGED`. Concretely, for each gate in `GateName` (§6):

```text
score(itr_N+1, gate)  >=  score(itr_N, gate)            for every gate
```

where `score` is a per-gate scalar described in §6 (e.g., for STABILITY the score is "consecutive successful boots / N"; for SECURITY it is the kspp checklist percentage). If any single gate's score regresses, the iteration is `DIVERGED` until the regression is fixed.

The monotonicity rule prevents an oscillation around a local optimum: without it, the pipeline could loop forever, alternating between two configurations that pass and fail different gates. With it, every iteration must be at least as good as the previous one across **all** gates; the only forward direction is strictly upward (or stable on already-perfect gates).

### §5.4 Termination

The pipeline terminates exactly when one of the following holds:

1. `convergence_state = CONVERGED` and all six gates `PASSED` → continue to §9 promotion.
2. `convergence_state = DIVERGED` → halt; emit `KERNEL_DIVERGED_REGRESSION` (`EXTENDED_60M`); operator must triage.
3. `convergence_state = ABANDONED` → halt; emit FOREVER evidence with the operator-supplied reason.
4. Any single gate returns `FAILED` or `TIMEOUT` and the operator chooses not to iterate → halt; emit `KERNEL_GATE_RESULT` (`STANDARD_24M`) for the failed gate; pipeline run total counter increments with `result=gate_failed`.

The pipeline does not have a hard iteration cap. A pipeline that takes 50 iterations to converge on a new hardware platform is not failing; it is doing the work. A pipeline that has not converged after a configurable budget is reported in telemetry (`kernel_pipeline_iterations_total`) but is not auto-abandoned; the operator decides when enough is enough.

## §6 Element 3 — Six machine-checkable acceptance gates (all-of)

### §6.1 `GateName` and `GateResult` enums

```proto
enum GateName {
  GATE_NAME_UNSPECIFIED = 0;
  STABILITY            = 1;
  SECURITY             = 2;
  HARDWARE_FIT         = 3;
  PERFORMANCE          = 4;
  REPRODUCIBILITY      = 5;
  RECOVERY_REHEARSAL   = 6;
}

enum GateResult {
  GATE_RESULT_UNSPECIFIED = 0;
  PASSED  = 1;
  FAILED  = 2;
  TIMEOUT = 3;
  SKIPPED = 4;
}
```

The six values of `GateName` are exhaustive. There is no "extra" gate. There is no "best-effort" or "cosmetic" gate. Every gate is machine-checkable against a quantitative threshold. The four values of `GateResult` are exhaustive — `PASSED`, `FAILED`, `TIMEOUT`, `SKIPPED`. There is no `WARNING`, no `PASSED_WITH_NOTES`. A gate either passes mechanically or it does not.

### §6.2 `STABILITY`

| Field     | Value                                                                                                                                                                          |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Statement | The kernel boots reliably and runs a sustained workload without panic, KASAN/UBSAN/lockdep complaint, or RCU stall.                                                            |
| Measure   | Consecutive successful boots count `N`; aggregate KASAN/UBSAN/lockdep complaint count over `T` of sustained workload; panic count over the boot+workload window.               |
| Threshold | `N >= 10` consecutive boots **AND** `T = 24 hours` sustained synthetic workload **AND** `kasan_count = 0 AND ubsan_count = 0 AND lockdep_count = 0` **AND** `panic_count = 0`. |
| Score     | `min(N/10, 1.0) * 0.5 + (24h_clean ? 0.5 : 0)`; binary thresholding on top.                                                                                                    |

### §6.3 `SECURITY`

| Field     | Value                                                                                                                                                                                                                                                                                                                 |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Statement | The kernel image meets a high-water mark on the Kernel Self-Protection Project (kspp) checklist and has the six AIOS-required hardening features active.                                                                                                                                                              |
| Measure   | (a) `kconfig-hardened-check` checklist score; (b) `/sys/kernel/security/lockdown` reports `confidentiality`; (c) signed-only modules (`CONFIG_MODULE_SIG_FORCE = y`); (d) kexec revoked (`CONFIG_KEXEC=n` or lockdown-blocked); (e) BPF restricted (`bpf_jit_harden=2`, unprivileged BPF off); (f) ftrace restricted. |
| Threshold | `kspp_score >= 95%` **AND** all six features active.                                                                                                                                                                                                                                                                  |
| Score     | `kspp_score / 100` × all-six-features-active boolean.                                                                                                                                                                                                                                                                 |

### §6.4 `HARDWARE_FIT`

| Field     | Value                                                                                                                                                                                                                                                                 |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Statement | The image drives the host's actual hardware: every device in the L8 hardware graph snapshot has a bound driver, and every required firmware blob is present and signed.                                                                                               |
| Measure   | (a) `unbound_device_count` from `/sys/bus/*/devices/` cross-referenced against the `HardwareGraphSnapshot`; (b) `missing_firmware_count` from the firmware blob set; (c) every present firmware blob's Ed25519 signature verifies against the vendor's catalog entry. |
| Threshold | `unbound_device_count == 0` **AND** `missing_firmware_count == 0` **AND** `unsigned_firmware_count == 0`.                                                                                                                                                             |
| Score     | `1.0 - (unbound + missing + unsigned) / total_devices`.                                                                                                                                                                                                               |

### §6.5 `PERFORMANCE`

| Field     | Value                                                                                                                                                                                                                                       |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Statement | The dedicated kernel does not regress measurably against the generic-kernel baseline on the four canonical workload classes.                                                                                                                |
| Measure   | (a) boot-time wall-clock (firmware exit → `STAGE_RECOVERY_SHELL_READY` equivalent); (b) `fio` filesystem p95 latency on a fixed mixed-IO workload; (c) `iperf3` single-stream network p95 throughput; (d) `vkmark` GPU init wall-clock p95. |
| Threshold | Each of the four measures within `≤ +10%` regression of the generic-kernel baseline taken in the same hardware graph snapshot.                                                                                                              |
| Score     | `min(baseline / observed, 1.0)` averaged over the four measures.                                                                                                                                                                            |

### §6.6 `REPRODUCIBILITY`

| Field     | Value                                                                                                                 |
| --------- | --------------------------------------------------------------------------------------------------------------------- |
| Statement | The build is deterministic. Three successive rebuilds with identical inputs produce three identical `bzImage` hashes. |
| Measure   | `hash(image_run_1) == hash(image_run_2) == hash(image_run_3)`.                                                        |
| Threshold | All three equal. No tolerance. A single byte-difference is a fail.                                                    |
| Score     | Boolean (1.0 if all three equal; 0.0 otherwise).                                                                      |

### §6.7 `RECOVERY_REHEARSAL`

| Field     | Value                                                                                                                                                                                                                                                                         |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Statement | The A/B fallback contract from S9.1 §11 holds end-to-end: deliberately breaking slot A's image causes the bootloader to fall back to slot B's generic kernel within the configured window, and the host reaches a recovery shell every time.                                  |
| Measure   | A scripted rehearsal: corrupt slot A's `bzImage` byte-pattern (the rehearsal tool, run inside `ISOLATED_SANDBOX`, writes a known-bad byte sequence to a copy then swaps the copy in for the rehearsal); reboot N times; count successful fallbacks to slot B; restore slot A. |
| Threshold | `5 / 5` successful fallbacks, each reaching `STAGE_RECOVERY_SHELL_READY` (per S9.1 §3.5) within 90 seconds of GRUB entry selection.                                                                                                                                           |
| Score     | `successful_fallbacks / 5`.                                                                                                                                                                                                                                                   |

### §6.8 The all-of rule

**All six gates must return `PASSED` for the image to be eligible for promotion. Any single `FAILED`, `TIMEOUT`, or `SKIPPED` blocks promotion. There is no human override of a gate failure.** This is the operational binding of INV-014 (no proof, no completion): a gate failure is the absence of proof; the absence of proof prohibits the status claim "this kernel image is fit to run as slot A".

The only path forward after a gate fail is **fix-and-rebuild**: change inputs, run a new iteration, run gates again. The operator may not "exception-grant" a kernel image past a gate failure. There is no `--force` flag. There is no `kernel.gate.override` action — and any attempt to introduce one is rejected by the policy kernel under INV-014 enforcement.

The operator can mark the pipeline `ABANDONED` at any time, which is an explicit decision to stop iterating. `ABANDONED` is not a way past a gate failure; it is a way to stop trying. The system continues running the previously-promoted slot A.

### §6.9 Why six gates

Why not five gates, why not seven gates? The closed set of six is the minimum cover of the constitutional risk surface for the kernel:

- STABILITY covers the failure mode "kernel runs but corrupts memory or stalls".
- SECURITY covers the failure mode "kernel runs but is exploitable through known classes".
- HARDWARE_FIT covers the failure mode "kernel runs but cannot drive the actual hardware".
- PERFORMANCE covers the failure mode "kernel runs and is correct but is unusable".
- REPRODUCIBILITY covers the failure mode "kernel runs today but cannot be reproduced for forensic, audit, or supply-chain reasons".
- RECOVERY_REHEARSAL covers the failure mode "kernel runs until it doesn't, and the fallback path was never proven".

Five gates would drop one of these and admit one whole failure class. Seven gates would split one of these into two correlated checks without adding a new failure class. Six is exactly the size of the closed list.

## §7 Element 4 — 3-AND filter for kernel additions

### §7.1 `AdditionFilterResult` enum

```proto
enum AdditionFilterResult {
  ADDITION_FILTER_RESULT_UNSPECIFIED = 0;
  ACCEPTED                      = 1;
  REJECTED_NO_INV_MAPPING       = 2;
  REJECTED_NOT_UPSTREAM         = 3;
  REJECTED_NOT_MEASURABLE       = 4;
}
```

A proposed kernel feature (a Kconfig option, an enabled subsystem, an integration with a userspace daemon) is admitted to the build if and only if **all three** filters are satisfied:

1. **`enforces_invariant`** — the feature mechanically enforces an existing AIOS L0 invariant. The proposal must cite an `INV-XXX`. A feature whose proposer cannot cite an existing invariant is rejected with `REJECTED_NO_INV_MAPPING`.
2. **`upstream_present`** — the feature is in upstream Linux mainline. The pinned `upstream_kernel_version` must contain the feature (the Kconfig option must be present in that tag's `Kconfig` tree). A feature requiring an out-of-tree patch is rejected with `REJECTED_NOT_UPSTREAM`.
3. **`runtime_measurable`** — there is a runtime probe that proves the feature is active. The probe is part of the feature's manifest and is run by the SECURITY or HARDWARE_FIT gate. A feature that cannot be measured at runtime is rejected with `REJECTED_NOT_MEASURABLE`.

The "AND" is constitutional. Two-out-of-three is not enough. A feature that is "really nice to have" but does not enforce an L0 invariant is rejected. A feature that enforces an invariant but is out-of-tree is rejected (the pipeline does not maintain a fork). A feature that enforces an invariant and is upstream but cannot be measured at runtime is rejected (we cannot tell whether it is on after deploy, so we cannot prove the gate, so INV-014 forbids the claim).

### §7.2 Accepted features (canonical applied table)

The following table is the closed list of kernel features the AIOS pipeline currently admits. Each row carries the feature's INV citation, the upstream Kconfig anchor, and the runtime measurement that proves the feature is active.

| Feature                                 | INV                           | Upstream config                                               | Measurement                                                                                 |
| --------------------------------------- | ----------------------------- | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| dm-verity on `/`                        | INV-004                       | `CONFIG_DM_VERITY` (≥ 4.x)                                    | `dmsetup status` reports verity-active for the root device                                  |
| IMA + EVM                               | INV-014                       | `CONFIG_IMA`, `CONFIG_EVM` (≥ 3.x)                            | `ima_appraise=enforce` in `/proc/cmdline` AND TPM PCR populated with expected boot hash     |
| Lockdown=confidentiality                | INV-013                       | `CONFIG_SECURITY_LOCKDOWN_LSM` (≥ 5.4)                        | `cat /sys/kernel/security/lockdown` shows `confidentiality`                                 |
| Signed-only modules                     | INV-013                       | `CONFIG_MODULE_SIG_FORCE` (≥ 3.x)                             | `cat /proc/sys/kernel/modules_disabled` AND signature verify on every load attempt          |
| kexec revoked                           | INV-013                       | `CONFIG_KEXEC=n` or lockdown-blocked at runtime               | `kexec_load(2)` returns `EPERM`                                                             |
| TPM measured boot + sealed vault root   | INV-018                       | `CONFIG_TCG_TPM`, `CONFIG_TCG_TIS`                            | `tpm2_unseal` succeeds against the expected PCR set; failure → vault refuses to unwrap      |
| Pinned eBPF correlator (PID → subject)  | INV-002 + INV-011             | `CONFIG_BPF_LSM`, `CONFIG_BPF_SYSCALL` (≥ 5.7)                | `bpftool prog show pinned /sys/fs/bpf/aios/pid_to_subject`                                  |
| Kernel-side evidence netlink emission   | INV-005                       | `CONFIG_AUDIT` plus AIOS netlink subscriber                   | netlink connector active; userspace cannot mask events without revoking the kernel-side mux |
| Stripped module list (defense-in-depth) | n/a — defense-in-depth        | n/a (the strip pass operates on the closed `KernelModuleSet`) | `lsmod` count vs `allowed_modules`; superset → fail                                         |
| Intel CET shadow stacks                 | n/a — hardware free hardening | `CONFIG_X86_USER_SHADOW_STACK` (≥ 6.6, hardware-conditional)  | `/proc/cpuinfo` reports `shstk` AND ELF binaries marked `shstk`                             |
| ARM MTE                                 | n/a — hardware free hardening | `CONFIG_ARM64_MTE` (hardware-conditional)                     | `/proc/cpuinfo` reports `mte` AND tag-fault counters available in `/proc/<pid>/status`      |

Notes on the n/a rows: features marked "defense-in-depth" or "hardware free hardening" do not enforce a single named L0 invariant directly; they are admitted because they reduce the size of the proof obligations on other features. The 3-AND filter therefore admits them under a special carve-out that requires the proposer to cite the _broader_ invariant they protect — for stripped modules, the broader protection is the trust-root chain (S11.1 plus L0 §3 INV-013 in spirit); for CET / MTE, it is the chain from INV-013 through hardware-enforced control-flow integrity. The carve-out is bounded: a row is admissible under the carve-out only if it is hardware-conditional or strictly subtractive (removes capability rather than adds). Adding a row under the carve-out requires a recovery-mode operation by a `HUMAN_USER` subject and is recorded with FOREVER `PIPELINE_DEFINITION_REPLACED` evidence.

### §7.3 Rejected features (canonical reject table)

The following table is the closed list of features the 3-AND filter has rejected and the corresponding `AdditionFilterResult`. The list is not exhaustive of the universe of possible additions; it is a record of considered-and-rejected proposals so that future proposers can see precedent.

| Feature                                      | Reject reason                                                                                                                                                                                               | Result                    |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------- |
| Custom scheduler (e.g., bespoke EEVDF tweak) | No INV mapping (premature optimization). The default upstream scheduler covers the workload classes AIOS targets.                                                                                           | `REJECTED_NO_INV_MAPPING` |
| Custom LSM (in-tree-style new module)        | Forces a fork. SELinux + AppArmor + Landlock together cover the LSM use cases AIOS needs. A custom LSM is rejected unconditionally — even if it were in-tree, it would still fail INV mapping for AIOS use. | `REJECTED_NOT_UPSTREAM`   |
| Custom syscall                               | No INV mapping; forces a fork; the userspace API surface is the right place for AIOS-specific operations (typed actions over RPC, not new syscalls).                                                        | `REJECTED_NO_INV_MAPPING` |
| Out-of-tree modules                          | Forces a fork-equivalent maintenance tax on every kernel rebase; rejected unconditionally regardless of the module's claimed value.                                                                         | `REJECTED_NOT_UPSTREAM`   |
| "Telemetry counter that we will add later"   | Not measurable today; admitting it would mean the SECURITY or HARDWARE_FIT gate cannot prove the feature is active.                                                                                         | `REJECTED_NOT_MEASURABLE` |

The reject table is published. A future proposer trying to revive a rejected feature must propose a mechanically-different feature; resubmitting an unchanged proposal is closed by the same row.

### §7.4 `SecurityTarget` enum

```proto
enum SecurityTarget {
  SECURITY_TARGET_UNSPECIFIED = 0;
  KSPP_BASELINE  = 1;
  KSPP_STRICT    = 2;
  KSPP_PARANOID  = 3;
}
```

| Value           | Threshold (kspp_score) | Notes                                                                                                                                                                                                                                                           |
| --------------- | ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `KSPP_BASELINE` | `>= 80%`               | Minimum admissible target. Activates dm-verity, IMA+EVM, lockdown=confidentiality, signed-only modules, kexec revoked, TPM measured boot. PERFORMANCE penalty: minimal.                                                                                         |
| `KSPP_STRICT`   | `>= 95%`               | Default for AIOS production hosts. Adds the pinned eBPF correlator and the kernel-side netlink evidence emission. Hardware-conditional features (CET, MTE) activated when present. PERFORMANCE penalty: small (`< 3%` regression typically).                    |
| `KSPP_PARANOID` | `>= 99%`               | Defense-grade hosts. Adds aggressive mitigations (slab hardening, randomization, fortify-source on every userspace mode-switch). PERFORMANCE penalty: real (`5–10%` regression on some workloads); admitted only when justified by a signed deployment posture. |

The pipeline rejects a `SECURITY_TARGET = SECURITY_TARGET_UNSPECIFIED` request at envelope validation (`ENVELOPE_VALIDATION_FAILED` per S10.1 §3.6). The default for a fresh AIOS install is `KSPP_STRICT`.

## §8 Element 5 — Build runs as AIOS typed action

### §8.1 The `kernel.build` action

The build is a typed `kernel.build` action under the L3 Capability Runtime (S10.1). It flows through the same fourteen-state `ActionLifecycleState` FSM (S10.1 §3.1) every other action does. There is no second governance plane.

Concrete bindings:

| Binding axis          | Value                                                                                                                                                                                                                                                                                                                             |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Action name           | `kernel.build`                                                                                                                                                                                                                                                                                                                    |
| Subject               | `_system:service:kernel-builder` (per S5.1 `SubjectKind` — `_system` scope, `service` kind). `is_ai = false`. The kernel builder service is a system identity; it is not an AI subject.                                                                                                                                           |
| `ActionDispatchKind`  | `ISOLATED_SANDBOX` (per S10.1 §3.2). The build runs under a `SandboxProfile` composed per S3.2 with a per-build-attempt enforcement boundary. AI subjects cannot influence the dispatch kind.                                                                                                                                     |
| `AdapterIOMode`       | `TYPED_PARAMETERS_ONLY` (per S10.1 §3.3). The build adapter accepts a typed `BuildRequest` proto and produces a typed `BuildResponse`. There is no shell input. There is no template substitution.                                                                                                                                |
| `AdapterStability`    | `STABLE`. The kernel-builder adapter is registered under STABLE stability (S10.1 §3.4) once the pipeline-definition is signed.                                                                                                                                                                                                    |
| Approval              | `kernel.build` is a non-interactive scheduled action triggered by L3 SGR or by an operator-initiated `kernel.refresh`. The dispatch path requires no per-invocation operator approval; the pipeline-definition itself is the standing approval. The promotion step (§9), separately, does require recovery-mode + human approver. |
| Sandbox profile       | A signed `SandboxProfile` named `prof_kernel_builder` that locks the build to a sealed builder workspace, denies network egress except to the pinned upstream mirror, denies access to anything outside the build tree.                                                                                                           |
| Verification (`S2.4`) | `verification_intent` includes the per-gate verification primitives. The action transitions `EXECUTING → VERIFYING` after the adapter returns success; the gates run in `VERIFYING`. A failed gate produces `EXECUTION_VERIFICATION_FAILED`.                                                                                      |
| Failure semantics     | A FAILED gate transitions the action to `FAILED` with `ExecutionFailureReason = VERIFICATION_FAILED`. The action does **not** automatically rollback — there is nothing to rollback (the build wrote into the builder workspace, not the host). The next iteration is a _new_ `kernel.build` action.                              |

### §8.2 The build cannot run with anything not in evidence

Every input to `Build` (§4.1) is content-addressed and traceable to a signed source:

- `hardware_graph_snapshot_id` is an L8 HDM artifact whose creation emitted `HARDWARE_GRAPH_SNAPSHOT_RECORDED` evidence.
- `aios_invariant_bundle_id` is an L0 invariant bundle whose load emitted `INVARIANT_BUNDLE_LOADED` (FOREVER, per L0 §6).
- `upstream_kernel_version`'s tarball is fetched from the AIOS-pinned upstream mirror; the fetch emits `KERNEL_PIPELINE_STARTED` (`STANDARD_24M`) and the tarball's content hash is verified against the signed mirror catalog.
- `module_set` is the closed list whose every modification emitted FOREVER `KERNEL_MODULE_SET_CHANGED` evidence (queued under §7.2 carve-out additions, recorded as `PIPELINE_DEFINITION_REPLACED`).
- Every `firmware_blob` carries a vendor Ed25519 signature and a content hash recorded against the firmware catalog.
- The `pipeline_definition_id` is itself signed by the AIOS root key.

A build whose input chain has any broken link (e.g., a hardware graph snapshot whose HDM evidence record is missing) fails envelope validation at S10.1 §5 step 1 (`ENVELOPE_VALIDATION_FAILED`). The build cannot run with anything that is not in evidence.

### §8.3 Each gate result is typed evidence

Every gate evaluation produces a `KERNEL_GATE_RESULT` evidence record (`STANDARD_24M`, see §15) with the following payload shape:

```text
KernelGateResultPayload {
  kernel_build_attempt_id: string             // "kbld_<ulid26>"
  kernel_image_id:         string             // "kimg_<full_blake3_64hex>"
  gate:                    GateName
  result:                  GateResult
  measurement:             google.protobuf.Struct  // closed schema per gate
  threshold:               google.protobuf.Struct
  evaluator_subject:       string             // "_system:service:kernel-builder"
  signed_at:               google.protobuf.Timestamp
  ed25519_signature:       bytes              // signed by kernel-builder over the canonical receipt bytes
}
```

The signature is by the `_system:service:kernel-builder` subject's signing key, registered in S5.2 vault. A forged gate result with a missing or mismatched signature is rejected at evidence-log append time (the evidence log requires every payload type to come from an authorised producer per S3.1 §4 final paragraph). Gate-result forgery is therefore a vault-key compromise problem, not a logic-error problem; the threat model at that level is handled by S5.2.

### §8.4 The recursion is closed coherently

The architectural property this gives AIOS is: **AIOS builds AIOS using AIOS**. The same Capability Runtime that dispatches a `pkg.install` is the runtime that dispatches `kernel.build`. The same evidence log that records a policy decision records a gate result. The same vault that issues an app's signing capability issues the kernel-builder's. The same recovery-mode requirement that gates `policy_bundle.replace` gates `kernel.promote_to_a`. There is no separate "build system" with weaker invariants.

The recursion is constructive, not vicious, because the invariant chain bottoms out at the AIOS root key and the firmware-pinned trust anchor (per S11.1 §1). Each builder loop produces an image that the _next_ boot's drift detector (§10) verifies against the _previous_ loop's vault-sealed expected hash. There is no point in the loop where the system has to "trust the result without proof".

## §9 Element 6 — A/B promotion FSM

### §9.1 `KernelImageState` enum

```proto
enum KernelImageState {
  KERNEL_IMAGE_STATE_UNSPECIFIED = 0;
  BUILDING            = 1;
  BUILT               = 2;
  GATING              = 3;
  GATE_PASSED         = 4;
  GATE_FAILED         = 5;
  A_PROMOTED          = 6;
  B_DEMOTED_TO_A      = 7;
  ROLLBACK            = 8;
  RETIRED             = 9;
}
```

| Value            | Meaning                                                                                                                                                                                                                                                                                                                          |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `BUILDING`       | The `kernel.build` action is `EXECUTING`. The image is being produced.                                                                                                                                                                                                                                                           |
| `BUILT`          | The image has been produced; `kernel_image_blake3` is computed; the image sits in the builder workspace.                                                                                                                                                                                                                         |
| `GATING`         | The six gates are running (or queued). Action lifecycle is `VERIFYING`.                                                                                                                                                                                                                                                          |
| `GATE_PASSED`    | All six gates returned `PASSED`. The image is eligible for promotion. The lifecycle action is `SUCCEEDED`.                                                                                                                                                                                                                       |
| `GATE_FAILED`    | One or more gates returned a non-`PASSED` result. The image is terminal until the next iteration (which is a new `kernel.build` attempt with a different `kbld_<id>`).                                                                                                                                                           |
| `A_PROMOTED`     | The image is installed at slot A. The vault's `current_a_kernel_image_hash` has been updated to this image's full BLAKE3. The previous A-slot image (if any) is still present at slot B as the new generic-slot backup, **OR** moved to `RETIRED` if a generic-fallback distribution image is being preserved at slot B.         |
| `B_DEMOTED_TO_A` | The dedicated kernel image has been validated for `>= N_validation_boots = 5` consecutive boots and the _previous_ slot-A image (always a dedicated kernel) has been moved to slot B as backup. The generic-fallback distribution image is preserved at a third "deep recovery" slot referenced by GRUB entry 1/3 per S9.1 §4.1. |
| `ROLLBACK`       | Slot A failed `N_rollback_boots = 2` consecutive boot attempts. The bootloader has automatically promoted the slot-B image. FOREVER evidence is emitted. The previous slot A is moved to `RETIRED`.                                                                                                                              |
| `RETIRED`        | The image is archived for forensic purposes. It is not bootable from any GRUB entry. The image bytes are preserved in `/aios/system/kernel/retired/` until the operator chooses to purge.                                                                                                                                        |

### §9.2 Allowed transitions (exhaustive)

```text
BUILDING ─▶ BUILT
BUILT ─▶ GATING
GATING ─▶ GATE_PASSED
GATING ─▶ GATE_FAILED
GATE_PASSED ─▶ A_PROMOTED                  (recovery-mode required)
A_PROMOTED ─▶ B_DEMOTED_TO_A               (after N_validation_boots consecutive successes)
A_PROMOTED ─▶ ROLLBACK                     (after N_rollback_boots consecutive boot failures; automatic)
B_DEMOTED_TO_A ─▶ ROLLBACK                 (same condition; automatic)
ROLLBACK ─▶ RETIRED
GATE_FAILED ─▶ RETIRED                     (operator-initiated archive)
A_PROMOTED ─▶ RETIRED                      (operator-initiated archive after a successor image is promoted)
B_DEMOTED_TO_A ─▶ RETIRED                  (operator-initiated archive after another image takes the slot)
```

All other transitions are forbidden. Notably, there is **no** `GATE_FAILED ─▶ A_PROMOTED` transition. There is **no** `RETIRED ─▶ A_PROMOTED` transition (a retired image cannot be reinstated; rebuild it instead, producing a fresh `kbld_<id>` and a fresh `kimg_<hash>`).

### §9.3 Promotion is recovery-mode-gated

The transition `GATE_PASSED ─▶ A_PROMOTED` is a recovery-mode operation per S9.1 §3.6 `RecoveryMutableScope = DEDICATED_KERNEL_PROMOTION`. The transition writes to `/aios/system/kernel/`, which is a recovery-only mutable path. The operator must:

1. Reboot into recovery mode (S9.1 §4.1 GRUB entry 2 or 3).
2. Authenticate with hardware credential (S9.1 §6).
3. Submit the `kernel.promote_to_a` typed action with the target `kernel_image_id`.
4. The Policy Kernel checks: `is_recovery_mode = true`, subject is a `HUMAN_USER` per INV-012/INV-013, the named image's `KernelImageState = GATE_PASSED`.
5. The action transitions to `SUCCEEDED`; vault is updated with the new `current_a_kernel_image_hash`; FOREVER `KERNEL_PROMOTED_TO_A` evidence is emitted.

A `kernel.promote_to_a` submitted from normal mode is hard-denied with `RecoveryRequiredForSystemMutation` (S2.3 hard-deny per INV-012). A `kernel.promote_to_a` submitted by an AI subject is hard-denied with `AISystemAdminBlocked` (INV-013) before recovery-mode is even considered.

### §9.4 Rollback is automatic

`A_PROMOTED ─▶ ROLLBACK` is **not** human-driven. The bootloader's boot-attempt counter (lives in GRUB state, per S9.1 §3.6 `RecoveryMutableScope.L1_BOOT_PARAMETERS`) tracks consecutive failed boots of slot A. When the counter reaches `N_rollback_boots = 2`:

1. GRUB selects entry 1 (slot B; generic fallback) on the next boot. (For a `B_DEMOTED_TO_A` host, slot B is the previous dedicated kernel; for a fresh-bootstrap host, slot B is the distribution generic.)
2. The newly-running B-slot image emits `KERNEL_ROLLBACK_PERFORMED` (FOREVER) at boot, naming the failed `kernel_image_id` and the rollback reason inferred from the previous boot's last evidence.
3. Vault's `current_a_kernel_image_hash` is updated to the new running image's hash by the boot path's identity service in degraded mode (the field is mutable in recovery; the rollback boot transitively counts as a recovery-influenced operation).
4. The failed image transitions to `RETIRED` and is preserved in `/aios/system/kernel/retired/`.

The rollback does not require an operator. INV-001 (recovery independent of L5) demands that the system can rescue itself from a failed dedicated kernel without any cognitive layer. The bootloader's counter is the entire mechanism.

## §10 Element 7 — Drift detection: kernel image is evidence subject

### §10.1 Every-boot observation

On every successful boot, the running kernel computes the BLAKE3 of its own loaded `bzImage` (the kernel has access to its own image bytes via the boot loader's hand-off region; this is a feature of the kernel-side measured-boot machinery from §7.2 `IMA + EVM`). The hash is emitted as an evidence record:

```text
record_type: KERNEL_IMAGE_OBSERVED
retention:   STANDARD_24M
payload: {
  observed_kernel_image_hash:   "kimg_<full_blake3_64hex>"
  slot:                          "A" | "B" | "RECOVERY"
  observed_at:                   timestamp
  pcr_attestation:               <bytes from TPM>
}
```

The observation runs at `STAGE_RECOVERY_SHELL_READY` equivalent in normal-mode boot (i.e., the userspace point at which the evidence log is available; see S9.2 for the normal-mode equivalent stage name).

### §10.2 Expected hash from vault

The expected hash is loaded from the L4 vault. The vault holds two fields per host:

```text
kernel/current_a_kernel_image_hash   →  the BLAKE3 of the image currently expected at slot A
kernel/current_b_kernel_image_hash   →  the BLAKE3 of the image currently expected at slot B
```

Both are sealed against the TPM's expected PCR set (per §7.2 row "TPM measured boot"). A vault that cannot unseal at boot indicates either a hardware substitution attack or a corrupted vault state and itself triggers `RecoveryEntryReason = VAULT_ROOT_KEY_UNAVAILABLE` per S9.1 §3.3.

The fields are written exclusively by:

- `kernel.promote_to_a` (recovery-mode operation; updates `current_a_kernel_image_hash`)
- `B_DEMOTED_TO_A` transition (recovery-influenced; updates both fields atomically)
- `ROLLBACK` automatic transition (boot-path-driven; updates `current_a_kernel_image_hash` to the now-running image)

There is no other write path.

### §10.3 Drift comparison

The drift check on every boot is:

```text
if observed_kernel_image_hash != expected_kernel_image_hash_for_active_slot:
    emit KERNEL_IMAGE_DRIFT_DETECTED FOREVER {
        observed:  observed_kernel_image_hash
        expected:  expected_kernel_image_hash_for_active_slot
        slot:      "A" | "B"
    }
    drop into recovery mode immediately
```

"Drop into recovery" reuses the S9.1 mechanism: the boot path force-reboots into a recovery boot with `RecoveryEntryReason = KERNEL_IMAGE_TAMPER_DETECTED`. **This entry reason is queued for L1's next-revision update of the `RecoveryEntryReason` enum**; the eight values currently defined in S9.1 §3.3 will become nine. (Until the enum lands, the implementation reuses `EVIDENCE_LOG_TAMPER_DETECTED` per S9.1 §3.3 as the closest existing value, recorded as such in evidence with a payload note that the actual cause is kernel image drift; this is a transitional posture documented in §17.)

### §10.4 What this means

The rule means an unauthorised kernel swap — by a hostile bootloader injection, by an evil-maid attacker physically rewriting `/boot/vmlinuz-aios`, by an attacker who acquired root on a previous boot and rewrote the file before reboot — is detected at the **first boot after the swap**, not after silent operation. The kernel itself becomes a participant in the evidence chain that it hosts. The recursion is constructive: the evidence chain rooted at the kernel-image hash gates further evidence emission, and the kernel image whose hash is checked is itself the runtime that emits evidence.

The threat this closes:

- An attacker with physical access who swaps the kernel image: detected at first reboot.
- An attacker with root who swaps the kernel image from userspace: detected at first reboot. The attacker's window is one boot — they get one normal-mode session before the next boot drops into recovery.
- A bootloader compromise (a hostile GRUB) that loads a different kernel without updating the file: detected at first boot — the running kernel's own self-measurement does not match the expected vault entry; the running kernel itself emits the drift record before the attacker's payload has a chance to mask the evidence netlink (because §7.2 row "Kernel-side evidence netlink emission" guarantees the kernel itself emits, not a userspace daemon).

The drift detection is not perfect — an attacker who can both swap the kernel **and** rewrite the vault's `current_a_kernel_image_hash` and resign it under the AIOS root key has bypassed the entire trust chain, not just this gate. That attacker is the L0/L11 threat model, not S9.3's. S9.3 closes the operational gap for the realistic single-axis attacks.

## §11 Element 8 — Maintenance is structural property

### §11.1 The `kernel.refresh` scheduled action

Maintenance is not a separate process. It is a typed `kernel.refresh` action scheduled by the L3 SGR. The action's contract:

```proto
message KernelRefreshRequest {
  // The cadence on which this refresh fires. Operator-configurable per
  // host. Default: trigger on every new upstream stable release tag (the
  // upstream cadence is typically every ~8 weeks).
  string cadence_spec = 1;       // "every-stable-release" or cron string

  // The SecurityTarget the refresh targets. Default: same as the
  // currently-promoted A image's target.
  SecurityTarget security_target = 2;
}
```

The action's behaviour:

1. SGR fires `kernel.refresh` per its scheduler.
2. The refresh resolves the latest pinned upstream stable tag from the AIOS-pinned mirror catalog.
3. It invokes `KernelBuilder.Build` with the new `upstream_kernel_version` and the closed input set otherwise unchanged from the current A image's inputs.
4. The full pipeline runs (Build + Convergence + Six Gates).
5. If all six gates pass, the produced image enters `GATE_PASSED`. Promotion to A still requires recovery-mode + human approver (§9.3) — `kernel.refresh` does **not** auto-promote. The host runs on the previous A image until the operator takes the recovery action.
6. If any gate fails, the host continues running the previous A image. `KERNEL_REFRESH_PIPELINE_FAILED` (`EXTENDED_60M`) is emitted naming the failed gate. The operator is alerted but no urgent action is required: the current A is still validated; the system is not in danger.

### §11.2 `KernelMaintenanceResult` enum

```proto
enum KernelMaintenanceResult {
  KERNEL_MAINTENANCE_RESULT_UNSPECIFIED = 0;
  CONVERGED_PROMOTED_PENDING_OPERATOR = 1;   // gates passed; image awaits recovery-mode promotion
  GATE_FAILED                          = 2;
  PIPELINE_ERROR                       = 3;
  ABANDONED_BY_OPERATOR                = 4;
}
```

Every `kernel.refresh` execution emits a `KERNEL_REFRESH_SCHEDULED` (`STANDARD_24M`) at start and exactly one of the four `KernelMaintenanceResult` values at finish, recorded in `KERNEL_REFRESH_PIPELINE_FAILED` (`EXTENDED_60M`) on `GATE_FAILED` or `PIPELINE_ERROR`, or in the build's success record on `CONVERGED_PROMOTED_PENDING_OPERATOR`.

### §11.3 The principle

Maintenance is automated. The schedule is structural. Humans only act when a gate fails or when a passed image is awaiting recovery-mode promotion. If the pipeline filter (§7) is strict — and the rejected list is long — gate failures are rare; maintenance becomes a quarterly-cadence drumbeat of "gates passed; please reboot into recovery to promote", not a heroic effort.

This eliminates the "do I have time to maintain a custom kernel" question that kills most custom-kernel projects. The pipeline is the discipline. The discipline does not depend on human heroism; it depends on the gates being well-defined and the schedule being kept.

## §12 Vocabulary appendix

All closed enums introduced or referenced by this contract, collected for reference. None admits an `OPEN` or `OTHER` value. Adding a value is a versioned spec change.

```proto
enum ConvergenceState {
  CONVERGENCE_STATE_UNSPECIFIED = 0;
  ITERATING = 1;
  CONVERGED  = 2;
  DIVERGED   = 3;
  ABANDONED  = 4;
}

enum GateName {
  GATE_NAME_UNSPECIFIED = 0;
  STABILITY            = 1;
  SECURITY             = 2;
  HARDWARE_FIT         = 3;
  PERFORMANCE          = 4;
  REPRODUCIBILITY      = 5;
  RECOVERY_REHEARSAL   = 6;
}

enum GateResult {
  GATE_RESULT_UNSPECIFIED = 0;
  PASSED  = 1;
  FAILED  = 2;
  TIMEOUT = 3;
  SKIPPED = 4;
}

enum AdditionFilterResult {
  ADDITION_FILTER_RESULT_UNSPECIFIED = 0;
  ACCEPTED                      = 1;
  REJECTED_NO_INV_MAPPING       = 2;
  REJECTED_NOT_UPSTREAM         = 3;
  REJECTED_NOT_MEASURABLE       = 4;
}

enum SecurityTarget {
  SECURITY_TARGET_UNSPECIFIED = 0;
  KSPP_BASELINE  = 1;
  KSPP_STRICT    = 2;
  KSPP_PARANOID  = 3;
}

enum KernelImageState {
  KERNEL_IMAGE_STATE_UNSPECIFIED = 0;
  BUILDING            = 1;
  BUILT               = 2;
  GATING              = 3;
  GATE_PASSED         = 4;
  GATE_FAILED         = 5;
  A_PROMOTED          = 6;
  B_DEMOTED_TO_A      = 7;
  ROLLBACK            = 8;
  RETIRED             = 9;
}

enum KernelMaintenanceResult {
  KERNEL_MAINTENANCE_RESULT_UNSPECIFIED = 0;
  CONVERGED_PROMOTED_PENDING_OPERATOR = 1;
  GATE_FAILED                          = 2;
  PIPELINE_ERROR                       = 3;
  ABANDONED_BY_OPERATOR                = 4;
}
```

## §13 Cross-spec dependencies

| Spec   | Direction | What this spec contributes / consumes                                                                                                                                                                                                                                                        |
| ------ | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| L0 §3  | consumer  | Cites INV-001 (recovery independent of L5), INV-004 (recovery boundary), INV-005 (evidence append-only), INV-013 (AI cannot system admin), INV-014 (no proof, no completion), INV-018 (vault no raw secret leak) as binding rules.                                                           |
| S9.1   | consumer  | Cites `RecoveryStage`, `RecoveryMutableScope = DEDICATED_KERNEL_PROMOTION` and `L1_BOOT_PARAMETERS`, A/B kernel slot semantics, GRUB entry layout. Queues a new `RecoveryEntryReason = KERNEL_IMAGE_TAMPER_DETECTED` for the next S9.1 enum revision (§17).                                  |
| S10.1  | consumer  | Binds `kernel.build` to `ActionLifecycleState`, `ActionDispatchKind = ISOLATED_SANDBOX`, `AdapterIOMode = TYPED_PARAMETERS_ONLY`, `AdapterStability = STABLE`, `ExecutionFailureReason = VERIFICATION_FAILED`. The build is an action; its lifecycle is the runtime's.                       |
| S3.2   | consumer  | The build runs under a `SandboxProfile` named `prof_kernel_builder`, composed per S3.2's five-source merge with the runtime safety floor active.                                                                                                                                             |
| S11.1  | consumer  | `KERNEL_CANDIDATE` packages from `AIOS_RECOVERY_REPO` are the input to `kernel.build` at the upstream-fetch step. `UpdateChannel = RECOVERY_CRITICAL` semantics gate the package install.                                                                                                    |
| S8.2   | consumer  | The hardware graph snapshot id consumed at §4.1 is produced by L8 HDM (S8.2's neighbour); the HARDWARE_FIT gate cross-references this snapshot.                                                                                                                                              |
| S3.1   | producer  | Thirteen new evidence record types queued for S3.1 Wave 7 (§15). All retention classes are explicit. Hash chain semantics are honoured; nothing in this spec rewrites or reorders evidence.                                                                                                  |
| S5.1   | consumer  | The `_system:service:kernel-builder` subject is registered under S5.1's `SubjectKind = service` discipline. The subject's `is_ai = false`. INV-013 is enforced regardless.                                                                                                                   |
| S5.2   | consumer  | `KEY_SIGN` capability for the kernel-builder's per-action signing key; `KEY_SIGN` for the AIOS root key used to sign the pipeline-definition; vault-sealed `current_a_kernel_image_hash` storage (the vault never reveals the raw hash to an AI subject; it only confirms equality on read). |
| L3 SGR | consumer  | The `kernel.refresh` scheduled action is dispatched by L3 SGR per its scheduler. SGR does not own the gate logic; it owns the cadence.                                                                                                                                                       |

## §14 Adversarial robustness

This section enumerates the realistic adversarial classes against the pipeline and the structural defense each is bounded by. The threat model is "single-axis attack against one stage of the pipeline at a time"; multi-axis attacks (kernel swap **and** vault key compromise **and** invariant bundle forgery) are L0/S11.1's threat model, not S9.3's.

### §14.1 Pipeline tampering

**Adversary:** modifies the pipeline-definition (the toolchain pin, the strip pass, the gate definitions).

**Bound:** The pipeline-definition is content-addressed and Ed25519-signed by the AIOS root key (§4.4). The build refuses to start if the signature does not verify. Replacing the pipeline-definition is a recovery-mode operation (S9.1 `RecoveryMutableScope = L1_BOOT_PARAMETERS`) and emits FOREVER `PIPELINE_DEFINITION_REPLACED` evidence (§15).

**Result:** A tampered pipeline-definition cannot run a build. A pipeline-definition replaced under recovery without authorisation cannot escape the FOREVER evidence trail. A subsequent boot's drift check (§10) will detect any image produced by an unauthorised pipeline-definition because the vault's `current_a_kernel_image_hash` was sealed against the previous pipeline.

### §14.2 Build determinism break

**Adversary:** introduces a non-deterministic build dependency (a buildtime random seed, a wall-clock-dependent header, a CPU-feature-dependent code path that resolves differently on rebuild hardware).

**Bound:** The REPRODUCIBILITY gate (§6.6) requires three rebuilds to produce identical hashes. A single byte difference fails the gate. The pipeline cannot promote. The non-determinism is detected by the pipeline itself, not by an after-the-fact audit.

**Result:** A non-deterministic build is observable mechanically. The operator triages the source of the non-determinism and either patches it or removes the offending input.

### §14.3 Gate-result forgery

**Adversary:** runs the build, writes a forged `KERNEL_GATE_RESULT` evidence record claiming all six gates passed, attempts promotion.

**Bound:** Every `KERNEL_GATE_RESULT` is signed by the `_system:service:kernel-builder` subject's signing key (§8.3). The evidence log refuses appends from a producer whose signature does not verify against the registered key for that record type. A forgery requires either: (a) compromising the kernel-builder's signing key (which is held in vault under L4 / S5.2 capability discipline; capture requires raw-secret-read, which is hard-denied for AI subjects under INV-018 and capability-gated for HUMAN_USER), or (b) compromising the AIOS root key (the constitutional collapse case, out of S9.3 scope).

**Result:** Gate-result forgery requires vault key compromise, which is upstream of S9.3.

### §14.4 Hostile mirror returning tampered upstream tarball

**Adversary:** controls a network path to the upstream mirror; serves a tampered Linux source tarball.

**Bound:** The fetch step (§8.2) verifies the tarball's BLAKE3 against a signed upstream-mirror catalog before any source is touched. The catalog is itself signed by the AIOS root key. A tampered tarball fails the content-hash check; the build aborts with `KERNEL_PIPELINE_STARTED` followed by no `KERNEL_BUILD_COMPLETED`; FOREVER evidence captures the mismatch.

**Result:** A network-level adversary cannot inject source. The mirror is a cache, not a trust authority; the trust is in the catalog's signature.

### §14.5 Evil-maid kernel swap

**Adversary:** has physical access. Swaps `/boot/vmlinuz-aios` for a tampered image while the host is powered off.

**Bound:** The `KERNEL_IMAGE_OBSERVED` record at first boot (§10.1) reports the BLAKE3 of the **actually-loaded** kernel. The expected hash is loaded from vault, sealed against the TPM's expected PCR set. An evil-maid swap changes the loaded image's BLAKE3 and is detected at first boot; `KERNEL_IMAGE_DRIFT_DETECTED` (FOREVER) is emitted; the system reboots into recovery automatically (§10.4).

**Result:** The attacker's window is one boot. They get no normal-mode session. The drift evidence is FOREVER and cannot be redacted (INV-005).

### §14.6 Insider with kernel-builder credential

**Adversary:** an operator with the `_system:service:kernel-builder` subject's credential (e.g., a compromised service account).

**Bound:** Even with the credential, the insider can produce a `KernelImage` and submit a `kernel.promote_to_a` action — but the action is hard-denied unless `is_recovery_mode = true` (INV-012). Recovery-mode requires a physical reboot into the recovery GRUB entry and a HUMAN_USER authentication. The kernel-builder service subject is not a HUMAN_USER under S5.1 / INV-013; an attempt to promote is rejected with `RecoveryRequiredForSystemMutation`. Even if the insider also has a HUMAN_USER credential and physical access, the promotion still emits FOREVER `KERNEL_PROMOTED_TO_A` evidence with the operator's canonical id; there is no path to silent promotion.

**Result:** Compromise of the kernel-builder service account does not yield a promotable image. The bound below this requires HUMAN_USER physical recovery + FOREVER evidence — the auditable case.

## §15 Telemetry contract

Bounded-cardinality metrics. Subject id is **never** a label. Image hash is **never** a label. Build attempt id is **never** a label. The label cardinality budget per metric is `<= 50` total label tuples across the host's lifetime (modest because the closed enums are small).

| Metric                              | Type    | Labels (closed)                                                    | Notes                                                                                                              |
| ----------------------------------- | ------- | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| `kernel_pipeline_run_total`         | counter | `result` ∈ {`converged_promoted`, `gate_failed`, `pipeline_error`} | Increments on each pipeline run terminal.                                                                          |
| `kernel_gate_evaluation_total`      | counter | `gate` ∈ `GateName` (6 values), `result` ∈ `GateResult` (4 values) | Cardinality budget: 6 × 4 = 24 label tuples maximum.                                                               |
| `kernel_image_observed_total`       | counter | `slot` ∈ {`A`, `B`, `RECOVERY`}                                    | Increments on every successful boot's `KERNEL_IMAGE_OBSERVED` emission.                                            |
| `kernel_image_drift_detected_total` | counter | none                                                               | Should remain at 0 in normal operation. Any non-zero value is a forensic event.                                    |
| `kernel_refresh_attempt_total`      | counter | `result` ∈ `KernelMaintenanceResult` (4 values)                    | Cardinality budget: 4 label tuples.                                                                                |
| `kernel_pipeline_iterations_total`  | counter | none                                                               | Counts iterations of the build-and-gate loop within a single pipeline run; used to detect "long-converging" hosts. |
| `kernel_addition_filter_total`      | counter | `result` ∈ `AdditionFilterResult` (4 values)                       | Counts proposed additions and which filter result they received.                                                   |

NEVER as labels (constitutionally):

- `kernel_image_hash` / `kernel_image_id` — high-cardinality, sensitive
- `kernel_build_attempt_id` — unbounded over time
- `subject_id` — sensitive; never a label per the L9 telemetry discipline

## §16 Evidence record types (queue for S3.1 Wave 7)

The thirteen evidence record types this spec emits, queued for S3.1 Wave 7 consolidation. Retention classes use the S3.1 vocabulary (`STANDARD_24M`, `EXTENDED_60M`, `FOREVER`).

| Record type                      | Retention      | Emitted on                                                                                                                                                         |
| -------------------------------- | -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `KERNEL_PIPELINE_STARTED`        | `STANDARD_24M` | A new `kernel.build` action transitions to `EXECUTING`; carries `kbld_<id>`, `pipeline_definition_id`, `hardware_graph_snapshot_id`, `upstream_kernel_version`.    |
| `KERNEL_BUILD_COMPLETED`         | `STANDARD_24M` | The build adapter returns success (image bytes produced); carries `kimg_<full_blake3>`.                                                                            |
| `KERNEL_GATE_RESULT`             | `STANDARD_24M` | Each gate evaluation; carries `GateName`, `GateResult`, measurement struct, threshold struct, kernel-builder Ed25519 signature.                                    |
| `KERNEL_CONVERGED`               | `STANDARD_24M` | The pipeline transitions `ConvergenceState ─▶ CONVERGED` (fixed-point reached); carries the final input-tuple hash.                                                |
| `KERNEL_DIVERGED_REGRESSION`     | `EXTENDED_60M` | A gate score regressed (monotonicity rule violated, §5.3); carries the regressing gate, previous score, current score.                                             |
| `KERNEL_PROMOTED_TO_A`           | `FOREVER`      | A `GATE_PASSED` image transitions to `A_PROMOTED` under recovery mode; carries operator canonical id, `kimg_<hash>`, predecessor hash.                             |
| `KERNEL_PROMOTED_TO_B`           | `FOREVER`      | The previous A image is moved to slot B (B_DEMOTED_TO_A transition or related); carries both image hashes.                                                         |
| `KERNEL_ROLLBACK_PERFORMED`      | `FOREVER`      | Bootloader auto-rollback after `N_rollback_boots` consecutive failures; carries failed `kimg_<hash>`, replacement `kimg_<hash>`, fail count.                       |
| `KERNEL_IMAGE_OBSERVED`          | `STANDARD_24M` | Every successful boot's running-kernel measurement; carries `kimg_<hash>`, slot, PCR attestation bytes.                                                            |
| `KERNEL_IMAGE_DRIFT_DETECTED`    | `FOREVER`      | Observed hash != expected hash on boot; carries observed and expected hashes, slot.                                                                                |
| `KERNEL_REFRESH_SCHEDULED`       | `STANDARD_24M` | An `kernel.refresh` action fires; carries cadence id, target upstream version.                                                                                     |
| `KERNEL_REFRESH_PIPELINE_FAILED` | `EXTENDED_60M` | A scheduled refresh's pipeline failed (`KernelMaintenanceResult ∈ {GATE_FAILED, PIPELINE_ERROR}`); carries failing gate or error class.                            |
| `PIPELINE_DEFINITION_REPLACED`   | `FOREVER`      | The pipeline-definition itself was replaced (recovery-mode operation per §4.4, §7.2 carve-out); carries old `kpdef_<id>`, new `kpdef_<id>`, operator canonical id. |

The thirteen records collectively cover: pipeline lifecycle (`STARTED`, `BUILD_COMPLETED`, `CONVERGED`, `DIVERGED_REGRESSION`); gate evaluation (`GATE_RESULT`); promotion lifecycle (`PROMOTED_TO_A`, `PROMOTED_TO_B`, `ROLLBACK_PERFORMED`); drift forensics (`IMAGE_OBSERVED`, `IMAGE_DRIFT_DETECTED`); maintenance lifecycle (`REFRESH_SCHEDULED`, `REFRESH_PIPELINE_FAILED`); and pipeline-definition mutation (`PIPELINE_DEFINITION_REPLACED`).

## §17 Worked examples

### §17.1 Initial bootstrap of dedicated kernel from a fresh AIOS install

A freshly-installed AIOS host is running on the generic distribution kernel at slot A. The operator decides to bootstrap the dedicated kernel.

```text
Step 1.  L8 HDM completes its initial hardware enumeration.
         Emits: HARDWARE_GRAPH_SNAPSHOT_RECORDED  (L8 record; outside this spec)
         Produces: hwsnap_<hex32>

Step 2.  Operator (HUMAN_USER, normal mode) requests a kernel.build action.
         Action subject: _system:service:kernel-builder (the operator triggers; the
                         service is the action subject)
         Inputs:  hardware_graph_snapshot_id = hwsnap_<hex32>
                  security_target            = KSPP_STRICT
                  aios_invariant_bundle_id   = invbundle_<active>
                  upstream_kernel_version    = "linux-6.6.42"
                  module_set                 = closed list (closed)
                  firmware_blob_set          = signed list (closed)
                  default_seccomp            = AIOS baseline
                  lsm_config                 = SELinux enforcing, AppArmor enforce, Landlock on, lockdown=confidentiality
         Action lifecycle:
           CREATED → POLICY_PENDING → APPROVED (standing approval from pipeline-definition)
                   → QUEUED (BACKGROUND queue) → EXECUTING

Step 3.  Build adapter runs in ISOLATED_SANDBOX with prof_kernel_builder.
         KernelImageState: BUILDING → BUILT
         Emits: KERNEL_PIPELINE_STARTED  STANDARD_24M
                KERNEL_BUILD_COMPLETED   STANDARD_24M
         Produces: kimg_<blake3_full_64hex>

Step 4.  KernelImageState: BUILT → GATING.
         Six gates run sequentially.
         For each:  emit KERNEL_GATE_RESULT  STANDARD_24M  with measurement vs threshold.

         STABILITY:           N=10 boots clean, 24h workload clean → PASSED
         SECURITY:            kspp 96.4%, lockdown=confidentiality, all six features active → PASSED
         HARDWARE_FIT:        unbound=0, missing_firmware=0, unsigned=0 → PASSED
         PERFORMANCE:         boot+9.2%, fio+6.7%, iperf+1.1%, vkmark+8.4% → PASSED (all ≤ +10%)
         REPRODUCIBILITY:     three rebuilds, identical BLAKE3 → PASSED
         RECOVERY_REHEARSAL:  5/5 fallbacks within 90s → PASSED

Step 5.  KernelImageState: GATING → GATE_PASSED.
         ConvergenceState:   ITERATING → CONVERGED
         Emits: KERNEL_CONVERGED  STANDARD_24M
         Action lifecycle:   VERIFYING → SUCCEEDED

Step 6.  Operator reboots into recovery mode (S9.1 §4.1 GRUB entry 2).
         Operator authenticates (S9.1 §6 hardware credential).
         Recovery shell up; STAGE_RECOVERY_ACTIVE.
         Operator submits kernel.promote_to_a action with kimg_<hash>.
         Policy Kernel: is_recovery_mode=true, subject is HUMAN_USER, image state = GATE_PASSED → ALLOW.
         Vault: current_a_kernel_image_hash = kimg_<hash> (sealed against TPM PCR set).
         KernelImageState: GATE_PASSED → A_PROMOTED.
         Emits: KERNEL_PROMOTED_TO_A  FOREVER

Step 7.  Operator commands recovery-reboot (REBOOT_TO_NORMAL per S9.1 §3.4).
         Boot path selects GRUB entry 0 (slot A; dedicated).
         The new dedicated kernel boots.
         At STAGE_NORMAL_RUNTIME_READY equivalent, drift check runs:
           observed = kimg_<hash>
           expected = kimg_<hash>     (from vault)
           match → no drift event.
         Emits: KERNEL_IMAGE_OBSERVED  STANDARD_24M  slot=A
         Bootloader counter: consecutive_a_successes++.
         When consecutive_a_successes >= N_validation_boots = 5:
           KernelImageState: A_PROMOTED → B_DEMOTED_TO_A (atomic update of slot B)
           Emits: KERNEL_PROMOTED_TO_B  FOREVER
```

End state: dedicated kernel running at slot A; previous distribution generic at slot B (`B_DEMOTED_TO_A`); deep recovery generic preserved for GRUB entries 1/3.

### §17.2 Periodic refresh against a new upstream stable that fails STABILITY

Six weeks later, upstream releases `linux-6.6.50`. The L3 SGR's `kernel.refresh` cadence fires.

```text
Step 1.  SGR fires kernel.refresh (cadence=every-stable-release).
         Emits: KERNEL_REFRESH_SCHEDULED  STANDARD_24M
                  cadence_id, target=linux-6.6.50

Step 2.  KernelBuilder.Build runs with the new upstream version.
         All other inputs unchanged.
         Build succeeds; produces kimg_<new_hash>.
         Emits: KERNEL_PIPELINE_STARTED, KERNEL_BUILD_COMPLETED  (both STANDARD_24M)

Step 3.  Six gates run.
         STABILITY: a regression in the upstream stable (a known issue with the
                    new release introduces a lockdep complaint under sustained
                    fio workload) shows up at hour 4 of the 24h sustained run.
                    lockdep_count = 17 → threshold violated (must be 0)
                    GateResult: FAILED
         Emits: KERNEL_GATE_RESULT  STANDARD_24M  gate=STABILITY result=FAILED measurement=...

Step 4.  Pipeline halts. KernelImageState: GATING → GATE_FAILED.
         KernelMaintenanceResult: GATE_FAILED.
         Emits: KERNEL_REFRESH_PIPELINE_FAILED  EXTENDED_60M  failing_gate=STABILITY

Step 5.  Host continues running the previously-promoted A slot
         (linux-6.6.42 dedicated). It is still validated. The operator
         is alerted but no urgent action is required.

Step 6.  When upstream issues the corrective stable (e.g. linux-6.6.51),
         SGR fires kernel.refresh again. The pipeline runs again. If gates
         pass, KernelMaintenanceResult = CONVERGED_PROMOTED_PENDING_OPERATOR;
         the operator does the recovery-mode promotion at their convenience.
```

End state: host continues on the validated `linux-6.6.42` image; refresh failure is recorded; no service interruption.

### §17.3 Evil-maid attack: attacker swaps boot image; first boot detects drift

The host is running with `kimg_X` at slot A. The operator powers off and steps away. An attacker with physical access boots from external media, mounts `/boot`, replaces `/boot/vmlinuz-aios` with `kimg_Y` (a tampered image with the same approximate file size). The attacker reboots the host and leaves.

```text
Step 1.  Host powers up. GRUB loads the file at /boot/vmlinuz-aios.
         The loaded image is kimg_Y (the tampered one).

Step 2.  The kernel boots. STAGE_NORMAL_RUNTIME_READY equivalent reached.
         Userspace point at which the evidence log is available.

Step 3.  Drift check runs:
           observed = kimg_Y          (the kernel measures itself via IMA + EVM)
           expected = kimg_X          (vault's current_a_kernel_image_hash)
           observed != expected
         Emits: KERNEL_IMAGE_OBSERVED      STANDARD_24M  slot=A  observed=kimg_Y
                KERNEL_IMAGE_DRIFT_DETECTED  FOREVER  observed=kimg_Y expected=kimg_X slot=A

Step 4.  Boot path force-reboots into recovery.
         RecoveryEntryReason = KERNEL_IMAGE_TAMPER_DETECTED  (queued for S9.1
           next-revision; transitionally recorded as EVIDENCE_LOG_TAMPER_DETECTED
           with payload note "actual cause: kernel image drift").
         GRUB selects entry 2 (recovery; dedicated) on the next boot.

Step 5.  Recovery boot proceeds per S9.1 §4.3.
         At STAGE_RECOVERY_SHELL_READY, operator authenticates.
         Operator inspects evidence log; sees KERNEL_IMAGE_DRIFT_DETECTED.
         Operator initiates ROLLBACK manually (kernel.rollback action) OR
         the bootloader's automatic counter has already triggered ROLLBACK
         on the previous failed-to-stay-up boot.

Step 6.  Slot B (the previous dedicated image, B_DEMOTED_TO_A from §17.1) is
         promoted; vault's current_a_kernel_image_hash is updated to
         match slot B's image. The tampered image is moved to RETIRED.
         Emits: KERNEL_ROLLBACK_PERFORMED  FOREVER
```

End state: host runs on the previous validated dedicated image (slot B promoted to A). The attacker's tampered image is in RETIRED for forensic analysis. The attacker had **zero** normal-mode sessions on the host. The drift evidence is FOREVER and cannot be redacted.

## §18 Open deferrals

| Item                                                     | Disposition                                                                                                                                                                                                                                                                                                                                    |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Custom LSM (in-tree-style new module)                    | **Rejected by §7 filter — out of scope permanently.** The closed list of LSMs (SELinux + AppArmor + Landlock) covers the AIOS use cases. Any future proposal for a new in-tree AIOS LSM is rejected at §7's `REJECTED_NOT_UPSTREAM` (until upstream) and `REJECTED_NO_INV_MAPPING` (without a fresh INV citation distinct from existing LSMs). |
| TPM remote attestation                                   | **Deferred to L8 HDM extension.** This spec uses TPM measured boot (§7.2) for sealing the vault root and for boot-time PCR observation. Network-side remote attestation (a remote verifier confirming the host's PCR set) is a future L8 HDM extension; it does not change the S9.3 contract.                                                  |
| Multi-architecture cross-compilation (x86_64 ⇄ aarch64)  | **Deferred.** The current pipeline assumes the build host architecture matches the target host. A cross-compile path requires the toolchain pin (§4.2) to additionally identify the target ABI; the work is mechanical but out of scope here.                                                                                                  |
| Per-group kernel variants                                | **Rejected — single A slot per host.** AIOS does not maintain per-group kernels. The host has one constitutional kernel; per-group differences live in userspace under S4.1's group namespace.                                                                                                                                                 |
| `RecoveryEntryReason = KERNEL_IMAGE_TAMPER_DETECTED`     | **Queued for S9.1 next-revision.** Currently §10.3 reuses `EVIDENCE_LOG_TAMPER_DETECTED` with a payload note. The next S9.1 enum revision adds the explicit value.                                                                                                                                                                             |
| Threshold or multi-party signing for the AIOS root key   | **Deferred to S11.1's existing deferral list.** S9.3's pipeline-definition signing inherits whatever signing scheme S11.1 settles on for the AIOS root.                                                                                                                                                                                        |
| Hardware attestation evidence in `KERNEL_IMAGE_OBSERVED` | **Initial value: PCR attestation bytes.** Richer attestation (signed quotes, runtime root-of-trust evidence) deferred to a future S9.3 refinement once L8 HDM expresses the contract.                                                                                                                                                          |

## §19 Acceptance criteria

- [ ] `KernelBuilder.Build` is a typed RPC under `aios.kernel.v1alpha1` with the input set defined in §4.1.
- [ ] The determinism contract (§4.2) requires three successive rebuilds to produce bit-identical `bzImage` hashes; a single byte difference fails REPRODUCIBILITY.
- [ ] `ConvergenceState` is a closed enum with four values: `ITERATING`, `CONVERGED`, `DIVERGED`, `ABANDONED`.
- [ ] The fixed-point criterion is defined formally on `hash(itr_N.<inputs>) == hash(itr_N+1.<inputs>)`.
- [ ] The monotonicity rule is enforced: any gate score regression yields `DIVERGED`.
- [ ] `GateName` is a closed enum with six values: `STABILITY`, `SECURITY`, `HARDWARE_FIT`, `PERFORMANCE`, `REPRODUCIBILITY`, `RECOVERY_REHEARSAL`.
- [ ] `GateResult` is a closed enum with four values: `PASSED`, `FAILED`, `TIMEOUT`, `SKIPPED`.
- [ ] Each gate has a 1-sentence Statement, a machine-readable Measure, and a Threshold.
- [ ] The all-of rule is binding: any non-`PASSED` result blocks promotion. There is no human override of a gate failure.
- [ ] `AdditionFilterResult` is a closed enum with four values: `ACCEPTED`, `REJECTED_NO_INV_MAPPING`, `REJECTED_NOT_UPSTREAM`, `REJECTED_NOT_MEASURABLE`.
- [ ] The 3-AND filter requires every accepted feature to satisfy `enforces_invariant`, `upstream_present`, AND `runtime_measurable`.
- [ ] The accepted features table (§7.2) lists at least 11 rows with INV citation, upstream config, and measurement.
- [ ] The rejected features table (§7.3) lists explicit precedent for at least 5 rejected proposals.
- [ ] `SecurityTarget` is a closed enum with three values: `KSPP_BASELINE`, `KSPP_STRICT`, `KSPP_PARANOID`.
- [ ] The build runs as a typed `kernel.build` action with `ActionDispatchKind = ISOLATED_SANDBOX`, `AdapterIOMode = TYPED_PARAMETERS_ONLY`, subject `_system:service:kernel-builder`.
- [ ] Each gate result emits typed `KERNEL_GATE_RESULT` evidence signed by the kernel-builder subject.
- [ ] `KernelImageState` is a closed enum with nine values; the allowed-transition table (§9.2) is exhaustive.
- [ ] Promotion to slot A (`GATE_PASSED ─▶ A_PROMOTED`) requires recovery-mode + HUMAN_USER + FOREVER `KERNEL_PROMOTED_TO_A` evidence.
- [ ] Rollback (`A_PROMOTED ─▶ ROLLBACK`) is automatic on `N_rollback_boots = 2` consecutive failed boots; FOREVER `KERNEL_ROLLBACK_PERFORMED` evidence.
- [ ] Every successful boot emits `KERNEL_IMAGE_OBSERVED` (`STANDARD_24M`).
- [ ] Drift detection (`observed != expected`) emits `KERNEL_IMAGE_DRIFT_DETECTED` (`FOREVER`) and triggers immediate recovery entry.
- [ ] `KernelMaintenanceResult` is a closed enum with four values; `kernel.refresh` runs the full pipeline and never auto-promotes.
- [ ] Thirteen evidence record types are queued for S3.1 Wave 7 (§16) with explicit retention classes.
- [ ] Telemetry conforms to §15: cardinality budgets respected; `kernel_image_hash`/`kernel_build_attempt_id`/`subject_id` are never labels.
- [ ] All three worked examples (§17.1 bootstrap, §17.2 refresh failure, §17.3 evil-maid drift) produce the specified evidence sequences.
- [ ] Adversarial robustness (§14) addresses pipeline tampering, determinism break, gate forgery, hostile mirror, evil-maid, and insider with kernel-builder credential.
- [ ] All cited cross-spec references resolve: L0 INV-001/004/005/013/014/018; S9.1 RecoveryStage / RecoveryMutableScope; S10.1 ActionLifecycleState / ActionDispatchKind / AdapterIOMode; S3.2 SandboxProfile; S11.1 PackageKind = KERNEL_CANDIDATE / RepositoryKind = AIOS_RECOVERY_REPO; S8.2 hardware graph snapshot; S3.1 RecordType / retention classes; S5.1 SubjectKind = service; S5.2 KEY_SIGN.

## §20 Status & evidence grade

Status: `REAL`

Evidence: E1 (artifact exists; structural contract complete; closed enums declared; allowed-transition tables exhaustive; cross-spec references resolved against in-repo specs). Higher grades (E2 build / E3 test / E4 e2e / E5 operational) are unattainable until a working `KernelBuilder` adapter is implemented, the gates are wired against a real Linux build, and a pipeline run produces a candidate image on real hardware. The mechanism in this spec is the contract those implementation phases must honour.

## See also

- [S9.1 — Recovery Boundary](01_recovery_boundary.md)
- [S10.1 — Capability Runtime gRPC](../L3_AIOS_SGR_Service_Graph_Runtime/03_capability_runtime_grpc.md)
- [S3.2 — Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [S11.1 — Repository Model + Trust Roots](../L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [S8.2 — GPU Resource Model](../L8_Network_Hardware_Devices/05_gpu_resource_model.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [L0 §3 — Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md)
- [L1 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
