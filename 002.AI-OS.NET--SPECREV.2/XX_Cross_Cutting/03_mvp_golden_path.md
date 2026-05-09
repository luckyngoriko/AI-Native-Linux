# MVP Golden Path Contract (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists; structural contract complete; trace walks every layer of the L0..L10 stack)                                                                                                                                                                                                                                                                                                                                                                              |
| Phase tag      | S0.3                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Layer          | XX Cross-Cutting (the trace consumes every layer L0..L10; the contract is owned by no single layer)                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Schema package | n/a — this contract is a trace and acceptance test, not a service                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Consumes       | L0.4 invariants INV-001..INV-024; S6.1 status taxonomy; S6.2 evidence grades; S9.1 recovery boundary; S1.3 object model; S2.1 query/view language; S4.1 namespace layout; S5.1 identity model; S2.3 policy kernel; S5.3 approval mechanics; S0.1 action envelope + lifecycle; S10.1 capability runtime gRPC; S2.4 verification grammar; S3.1 evidence log; S7.1 surface composition; S7.4 KDE renderer; S13.1 cognitive core (intent flow); S1.1 capability translator; S3.2 sandbox composition; S11.1 repository model |
| Produces       | the canonical 18-step trace; the binary acceptance checklist that defines "MVP achieved"; three additional traced scenarios (operator approves AI proposal; operator initiates recovery; AI tries to install a package directly and fails closed); three worked failure traces; the open-deferral list for MVP; a comprehensive cross-spec dependency map for audit                                                                                                                                                      |

## §1 Purpose

This contract is the **self-validation test of the AIOS rev.2 specification**. It walks the Rev.1 §22 golden path step-by-step through every constitutional layer of AIOS, and at every step it cites the precise sub-spec section that answers the step's question. The contract has a single binary acceptance criterion: **for every step in the canonical trace, the spec must give an unambiguous answer**. If any step has a gap, the gap is a finding for the audit phase and is documented in §6 of this contract as `GAP-§Step.X`.

This is the **executable test of the spec itself**. Without it, "the spec is implementable" is opinion. With it, "the spec is implementable" becomes a checkable property: any reader can walk this trace, verify each citation, and either close the test or surface a concrete gap.

This contract is **Tier 1 foundational**: without it the spec has no self-validation criterion; with it the audit phase has a single map that points at every spec contract the MVP touches and at every gap the MVP would expose.

The Rev.1 §22 golden path text:

```text
Boot from recovery-safe root → mount /aios → create a versioned AIOS-FS object →
resolve it through a semantic view → run one verified typed system action →
record the full evidence chain → show the result in a renderer.
```

The seven phases of that path are mapped, in this contract, to 18 mechanical steps tracing through L0 → L1 → L2 → L4 → L3 → L9 → L7. Each step has inputs, outputs, INV citations, evidence record types, failure modes, and an acceptance check. The trace as a whole forms one signed evidence DAG anchored on `correlation_id` (S0.1 §3.5) and `parent_receipt_id` linkage (S3.1 §5.2).

## §2 Scope

This contract **defines**:

1. The canonical golden path scenario (§3) and the operator subject that drives it.
2. The 18-step trace (§4), with one numbered subsection per step listing layer + sub-spec + section, inputs, outputs, INV citations, evidence record type emitted, failure modes, and the acceptance check.
3. The binary acceptance checklist (§5) that defines "MVP golden path achievable".
4. The honesty gates (§6) — for each step, an explicit declaration of whether it is fully specified, partially specified, or has a gap, with `GAP-§Step.X` placeholders for any genuine gaps surfaced during the walk.
5. Three additional traced scenarios (§7) showing breadth: operator approves an AI proposal; operator initiates recovery; AI agent tries to install a package directly and fails closed.
6. The open deferrals list (§8) — what is **out of scope** for the MVP golden path.
7. The MVP acceptance test as a verifiable artifact (§9): when AIOS is implemented, the test runs the canonical trace and passes iff every declared output and evidence record is produced and the trace forms a single signed DAG within an operator-latency budget.
8. The cross-spec dependency map (§10) — a comprehensive table tracing every step's dependencies. This is the audit map for the spec.
9. Three worked failure traces (§11) covering envelope-validation failure, AI-on-INTERACTIVE-queue silent downgrade, and orphan-chunk GC after partial write.
10. The status + evidence grade (§12).

This contract **does not** define:

- The MVP implementation plan, build system, or test harness — out of scope per the user's "no implementation plans for AIOS spec phase" directive.
- The `correlation_id` algorithm or the trace-collection wiring — owned by S0.1 §3.5 and §9.1.
- The runtime mechanics of any individual layer — owned by the cited sub-specs.
- The visual treatment of the renderer artifact — owned by S7.3 visual language and S7.4 KDE renderer.
- A cross-host or multi-host federation trace — out of scope for Rev.2 MVP.

This contract is a **trace specification**, not a service. It carries no proto IDL, no gRPC surface, no schema package. It carries citations.

## §3 The canonical golden path scenario

### §3.1 Scenario statement

The scenario this contract traces:

> Operator (`HUMAN_USER` subject `family:alice`) wants to record a daily journal entry: "I went hiking today." The system creates a versioned AIOS-FS object under `/aios/groups/family/users/alice/journal/`, makes it visible through the journal view, and renders confirmation in the operator's KDE evidence viewer.

This is the smallest end-to-end use of every constitutional layer of AIOS:

- **Operator-driven**, not AI-driven (HUMAN_USER subject; INV-002 not engaged on the actor axis).
- **Low-risk** (private-to-user write; no system-scope mutation; no recovery requirement).
- **Deterministic** (no agent reasoning loop; the operator types an exact instruction; the Capability Translator (S1.1) has a direct match in the catalog for `aios.fs.write` against a personal namespace path).
- **Observable** (the renderer must show the result; the evidence chain must be traversable from the renderer back to the action).

Choosing this scenario over an AI-mediated scenario keeps the MVP self-validation focused on the constitutional spine (L0 → L1 → L2 → L3 → L4 → L7 → L9). The AI-mediated path is exercised separately in §7 scenario A.

### §3.2 Subject and session

| Field                 | Value                                                                                                                                                                              |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Canonical subject id  | `family:alice`                                                                                                                                                                     |
| `SubjectKind`         | `HUMAN_USER` (S5.1 §3)                                                                                                                                                             |
| `is_ai`               | `false` (constitutionally bound at registration per S5.1 I6)                                                                                                                       |
| `primary_group_id`    | `family`                                                                                                                                                                           |
| `is_recovery_mode`    | `false` (NORMAL boot path; recovery is exercised separately in §7 scenario B)                                                                                                      |
| `SessionClass`        | `INTERACTIVE` (S5.1 §8.1) — operator is at the local KDE console, authenticated with strong credentials                                                                            |
| Authentication factor | hardware key (FIDO2) or password+WebAuthn (S5.1 §3 default for `HUMAN_USER`)                                                                                                       |
| Subject signature     | Ed25519 over canonical subject record by the L4 identity service public key (S5.1 I1) — present on the envelope's `request.subject` and validated by Capability Runtime at receipt |

The session is the prerequisite for everything that follows. A failure to authenticate before step 5 means the golden path does not start; the system is in `STAGE_RECOVERY_SHELL_READY` (recovery boot path) or in a pre-login NORMAL surface. The trace below assumes a successful login at step 5.

### §3.3 Action shape

The typed action that the operator's request becomes:

| Field                                 | Value                                                                                                                                                              |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `request.action`                      | `aios.fs.write` (dotted name from L5 capability catalog; S0.1 §4.2)                                                                                                |
| `request.target.path`                 | `/aios/groups/family/users/alice/journal/2026-05-11.md`                                                                                                            |
| `request.target.scope`                | `USER` (S4.1 §8 namespace resolution)                                                                                                                              |
| `request.target.group_id`             | `family`                                                                                                                                                           |
| `request.target.user_id`              | `alice`                                                                                                                                                            |
| `request.target.user_reserved`        | inferred from path resolution as `USR_HOME`-adjacent personal scope (the `journal/` subfolder lives under `users/alice/` — see §4.9 for the exact resolution path) |
| `request.target.content`              | the bytes `"I went hiking today.\n"` (UTF-8)                                                                                                                       |
| `request.subject`                     | `family:alice`                                                                                                                                                     |
| `request.reason`                      | `"daily journal entry"` (operator-supplied; ≤1024 chars per S0.1 §4.5)                                                                                             |
| `request.environment`                 | `LOCAL` (host-bounded; no LAN exposure)                                                                                                                            |
| `request.risk.destructive`            | `false`                                                                                                                                                            |
| `request.risk.privileged`             | `false` (writing to operator's own private namespace; no elevation needed)                                                                                         |
| `request.risk.network_exposure`       | `false`                                                                                                                                                            |
| `request.risk.secret_access`          | `false`                                                                                                                                                            |
| `request.risk.recovery_path_affected` | `false`                                                                                                                                                            |
| `request.verification`                | one `aiosfs.pointer` intent per S2.4 §4.1 (verifies the new pointer references the staged version)                                                                 |
| `request.dry_run`                     | `LIVE` (S0.1 §4.10)                                                                                                                                                |
| `request.privacy_class`               | `PRIVATE_TO_USER` (S1.3 §4.1) — bounded to the user's personal scope                                                                                               |

### §3.4 Object shape

The AIOS-FS object the action creates:

| Field             | Value                                                                                       |
| ----------------- | ------------------------------------------------------------------------------------------- |
| `object_id`       | `obj_<ULID>` (S1.3 §3.1)                                                                    |
| `kind`            | `FILE` (S1.3 §5)                                                                            |
| `created_by`      | `family:alice`                                                                              |
| `privacy_class`   | `SENSITIVE` floor (S1.3 §4.1 default if `PRIVATE_TO_USER` policy tag does not raise)        |
| `lifecycle_state` | `ACTIVE`                                                                                    |
| `policy_tags`     | `["personal", "journal"]`                                                                   |
| `metadata.name`   | `"2026-05-11.md"`                                                                           |
| `metadata.mime`   | `"text/markdown"`                                                                           |
| `scope_binding`   | `(USER, family, alice)` per S1.3 §21.1 / S4.1 §12.2                                         |
| Initial version   | `ver_<ULID>`, single chunk `chk_<BLAKE3-256-hex>` of the journal text bytes (S1.3 §6, §7.1) |
| Pointer           | `ptr_<ULID>`, kind = `CURRENT`, points at the initial version (S1.3 §8.1)                   |

### §3.5 Trace anchor

All evidence records, sub-spec lifecycles, and renderer notifications produced during the scenario share a single `correlation_id = corr_<ULID>` set at envelope creation (step 10) and propagated through every downstream lifecycle per S0.1 §3.5. The action's `action_id = act_<ULID>` is the per-envelope identity. Steps 16–17 reconstruct the full chain by querying L9.1 for `correlation_id = corr_<ULID>`.

## §4 The 18-step trace

Each step below is a self-contained micro-spec. The format is identical for every step:

- **Layer + sub-spec + section** — where the step is owned in the rev.2 sub-spec body.
- **Inputs** — what the step receives.
- **Outputs** — what the step produces.
- **INV citations** — the constitutional invariants from L0.4 that bind this step.
- **Evidence record type** — the S3.1 RecordType emitted (with retention class).
- **Failure modes** — what can go wrong; behaviour on failure; deferred-spec pointer if the failure surface is queued elsewhere.
- **Acceptance check** — the binary "the step is complete iff X" predicate.

The 18 steps are grouped into two phases: boot phase (steps 1–6, cited from S9.1 + S9.2) and action phase (steps 7–18, operator-driven).

### §4.1 Step 1 — Host boot, kernel selection, kernel load

**Layer + sub-spec + section.** L1 — S9.1 §4.1 (GRUB entries) + §3.5 (`RecoveryStage.STAGE_PRE_KERNEL`, `STAGE_KERNEL_LOADED`).

**Inputs.** Power-on signal; firmware/UEFI environment; signed GRUB configuration; the GRUB state file at `/aios/system/boot/state` (S9.1 §4.2) with `consecutive_normal_boot_failures = 0`.

**Outputs.** GRUB selects entry 0 (`AIOS Normal`, `kernel = /boot/vmlinuz-aios; aios.mode=NORMAL`) per the boot decision logic in S9.1 §4.2. The dedicated kernel image is loaded; `aios.mode=NORMAL` is parsed from the kernel command line. For Rev.2 MVP, the dedicated kernel pipeline is `DEFERRED` per §8; in practice the MVP boots the generic fallback kernel (entry 1) but the path through the FSM is identical except for the binary loaded.

**INV citations.**

- INV-001 (recovery independent of L5) — the boot path does not invoke any L5 component to reach a usable state. The kernel command line carries `aios.mode`; no AI is involved.
- INV-007 (layer downward dependency) — L1 has no dependency on L5; the boot path is L1-only.

**Evidence record type.** None at this step. Evidence emission begins at step 2 once L0 governance is up.

**Failure modes.**

- Firmware-level fallback: handled by S9.1 §3.5 stage table — failure at `STAGE_PRE_KERNEL` drops to firmware fallback; out of scope.
- Kernel image corrupt: A/B kernel fallback per S9.1 §11; the alternate kernel slot is selected and the boot continues with the fallback image.
- Three consecutive normal-boot failures: the GRUB decision logic (S9.1 §4.2) auto-routes into the recovery path — not the golden path. The scenario assumes a healthy host.

**Acceptance check.** The step is complete iff the kernel image is loaded and the kernel command line carries `aios.mode=NORMAL`.

**Honesty.** Fully specified — S9.1 §4.1, §4.2, §3.5 cover this completely.

### §4.2 Step 2 — STAGE_L0_GOVERNANCE_READY: invariant bundle loaded

**Layer + sub-spec + section.** L0 — L0.4 §4 (invariant bundle `invbundle_<hex>`); transition observed at S9.1 `RecoveryStage.STAGE_L0_GOVERNANCE_READY` (S9.1 §3.5). NORMAL boot has the same stage shape per S9.2 (deferred but the L0 load happens at boot time regardless of mode).

**Inputs.** The signed invariant bundle file at `/aios/system/governance/invariants/<bundle>` (per S9.1 §3.6 `RecoveryMutableScope.INVARIANT_BUNDLE`); the AIOS root Ed25519 public key embedded in `/`.

**Outputs.** The 24-entry invariant catalog (L0.4 §3 INV-001..INV-024) is loaded into the governance service; the bundle's Ed25519 signature is verified against the AIOS root key (L0.4 §4); the governance service enters NORMAL mode (all 24 invariants active).

**INV citations.**

- L0.4 I1 (closed list) — the catalog is the closed 24-entry enum.
- L0.4 I3 (invariants are signed) — bundle signature verification is the gate.
- L0.4 I5 (violations are FOREVER evidence) — any subsequent violation emits `TAMPER_DETECTED` with `invariant_id` populated.

**Evidence record type.** `INVARIANT_BUNDLE_LOADED` (FOREVER retention; declared by L0.4 §6 cross-spec touch-up to S3.1).

**Failure modes.**

- Signature failure: governance service enters degraded mode (only INV-001 + INV-002 active per L0.4 §4); the boot continues but every higher-layer mutation is denied. The golden path **does not start** in degraded mode; the scenario assumes a valid bundle.
- Parse failure: identical fail-closed behaviour as signature failure.
- Bundle missing: identical fail-closed behaviour; the recovery path (S9.1 §3.3 `RecoveryEntryReason.INVARIANT_BUNDLE_SIGNATURE_FAILURE`) is engaged on the next boot.

**Acceptance check.** The step is complete iff the governance service reports `degraded_mode = false` and `active_invariants = 24` and the `INVARIANT_BUNDLE_LOADED` evidence record is appended.

**Honesty.** Fully specified — L0.4 §4 + S9.1 §3.5 cover the load path; the FOREVER record type is queued in S3.1 (Wave 5+ table).

### §4.3 Step 3 — STAGE_L4_DEGRADED_READY for NORMAL boot — identity bundle loaded

**Layer + sub-spec + section.** L4 — S5.1 (identity bundle `idbundle_<hex>`, S5.1 I8); S9.1 `RecoveryStage.STAGE_L4_DEGRADED_READY` is the recovery analogue. In NORMAL boot, the equivalent stage is `STAGE_NORMAL_RUNTIME_READY` (deferred to S9.2). The structural contract is the same: the identity bundle is loaded and signature-verified.

**Inputs.** The signed identity bundle at `/aios/system/identity/<bundle>` (per S4.1 §3 `SystemReservedName.SYS_IDENTITY` — note: stored under the catalog-fixed `system/` namespace).

**Outputs.** The L4 identity service has the bundle loaded; `_system` scope subjects (constitutional) and group-registered subjects are resolvable. In NORMAL mode the full set is available; the degraded-mode subset (only `_system` subjects) is reserved for recovery boot.

**INV citations.**

- S5.1 I8 (identity bundle is signed and versioned) — load gate.
- S5.1 I2 (`_system` is constitutional) — `_system` subjects come from the bundle, not from runtime registration.

**Evidence record type.** Bundle-load evidence is queued in S3.1 (per the L4 identity refinement; the canonical name in Wave 6 is `IDENTITY_BUNDLE_LOADED` at FOREVER retention — see S3.1 §25).

**Failure modes.**

- Bundle signature failure: identity service enters degraded mode (only `_system` constitutional subjects); the boot transitions into the recovery path on next boot via S9.1 §3.3 `RecoveryEntryReason.IDENTITY_BUNDLE_FAILURE`.
- Bundle missing: identical to signature failure.

**Acceptance check.** The step is complete iff the identity service reports `degraded_mode = false` and the `family` group + `family:alice` subject are resolvable.

**Honesty.** Partially specified at the boot-stage level. The recovery analogue (`STAGE_L4_DEGRADED_READY`) is fully specified in S9.1 §3.5; the NORMAL analogue (`STAGE_NORMAL_RUNTIME_READY`) is owned by S9.2 which is `DEFERRED` per the master index. **GAP-§Step.3 (minor):** S9.2 first-boot installer / NORMAL boot stage FSM is `DEFERRED`; the structural shape is implied by the recovery FSM but is not contract-grade. This is queued for a follow-up sub-spec; not a blocker for the MVP self-validation because the boot phase is testable with the fallback generic-kernel path.

### §4.4 Step 4 — `/aios` mounted (AIOS-FS authoritative store)

**Layer + sub-spec + section.** L2 — S2.2 §D9 (RocksDB primary + SQLite metadata + Tantivy lexical) per the design decisions; S1.3 §13 (`/aios` layout). The mount sequence in NORMAL mode is the symmetric analogue of S9.1 §5.1's recovery row "`/aios`: NOT MOUNTED" — in NORMAL mode `/aios` IS mounted via the AIOS-FS userspace authoritative store (FUSE projection or portal projection per S2.2).

**Inputs.** The AIOS-FS volume identified by the host's mount table; the RocksDB column families on the underlying block device; the operator's filesystem encryption keys (LUKS/dm-crypt unlocked at boot or unsealed via TPM per S2.2 §D10).

**Outputs.** The `/aios/...` projection is live; the AIOS-FS object store is reachable; the namespace catalog (S4.1 §13 determinism contract) is loaded with `nscat_<hex>` version stamped and ready to resolve paths.

**INV citations.**

- INV-004 (recovery boundary preserved) — `/aios` is the AI-native root, distinct from `/` and `/root`.
- INV-007 (downward dependency) — L2 mount uses only L1 + L0.

**Evidence record type.** `AIOSFS_MOUNTED` (queued in S3.1 — exact name owned by S9.2 / S2.2; the L2 storage subsystem emits `STORAGE_INITIALIZED` at boot per S2.2's lifecycle when refined).

**Failure modes.**

- Mount failure: S9.1 §3.3 `RecoveryEntryReason.AIOSFS_ROOT_UNRESOLVABLE` — recovery path on next boot.
- Storage corruption: AIOS-FS WAL replay (S2.2 §D10) recovers; persistent corruption forces recovery.

**Acceptance check.** The step is complete iff `mount` reports `/aios` rw, the namespace resolver returns `nscat_<hex>` for the active catalog, and a probe `ResolvePath("/aios/groups/family")` returns a typed `NamespacePath{ scope=GROUP, group_id="family" }` per S4.1 §8.2.

**Honesty.** Partially specified. **GAP-§Step.4 (minor):** the boot-time evidence record name for AIOS-FS mount is queued in S3.1 but not yet contract-named. Tracked under S9.2 deferral; not a blocker.

### §4.5 Step 5 — STAGE_RECOVERY_SHELL_READY analogue: NORMAL session — operator authenticates

**Layer + sub-spec + section.** L4 — S5.1 §3 (authentication mechanisms for `HUMAN_USER`); S5.1 §8 (`SessionClass.INTERACTIVE`). The KDE login surface that delivers the auth prompt is owned by S7.4 (KDE renderer recovery + normal session bringup; S7.4 §6).

**Inputs.** Operator at the local KDE console; password+WebAuthn or hardware key; the L4 identity service from step 3.

**Outputs.** A signed `Subject` record for `family:alice` with:

- `canonical_subject_id = "family:alice"`
- `SubjectKind = HUMAN_USER`
- `is_ai = false`
- `primary_group_id = "family"`
- `SessionClass = INTERACTIVE`
- `is_recovery_mode = false`
- `expires_at = authenticated_at + <session TTL>` (TTL owned by S5.1 — for INTERACTIVE the TTL is operator-policy-driven; default per L4 identity refinement)

**INV citations.**

- S5.1 I1 (subject is unforgeable) — the signed `Subject` record is the gate.
- S5.1 I6 (`is_ai` set by identity service) — `family:alice` is constitutionally a HUMAN_USER.

**Evidence record type.** `IDENTITY_SUBJECT_AUTHENTICATED` (queued in S3.1 — exact name owned by S5.1 §11 evidence vocabulary at refinement; default retention `STANDARD_24M`).

**Failure modes.**

- Authentication failure: operator stays at the login surface; no `Subject` record issued; the golden path does not advance.
- Identity service degraded: only `_system` subjects available; `family:alice` is unresolvable; operator must boot recovery.

**Acceptance check.** The step is complete iff the L4 identity service has issued a signed `Subject` record for `family:alice` with `SessionClass = INTERACTIVE`, `is_ai = false`, and a non-expired session.

**Honesty.** Partially specified. The S5.1 evidence-record vocabulary is queued for S5.1 refinement; the structural contract (subject record, signing, session class) is fully specified.

### §4.6 Step 6 — NORMAL mode active: L3 SGR + L9.1 evidence log + KDE renderer come up

**Layer + sub-spec + section.** L3 — S10.1 (Capability Runtime online); L9 — S3.1 (Evidence Log online + accepting Append); L7 — S7.4 (KDE renderer attached to the operator's session via Plasma + KWin).

**Inputs.** Step 4 (`/aios` mounted) + Step 5 (operator authenticated).

**Outputs.**

- L3: `aios.runtime.v1alpha1.CapabilityRuntime` accepting `SubmitAction` calls; the registered adapter set includes `adapter:aios:fs:1.0.0` (the AIOS-FS adapter — registration via `runtime.adapter.register` per S10.1 §10).
- L9: `aios.evidence.v1alpha1.EvidenceLog` accepting `Append` (S3.1 §17); the hash chain is in a verified state (last-segment signature OK per S3.1 §11.4).
- L7: KDE Plasma session is up; the AIOS chrome `wlr-layer-shell overlay` layer is live (S7.4 §3.1, §3.2); the operator's evidence-viewer app is launchable from KRunner or the system tray (S7.4 §6.1, §6.2).

**INV citations.**

- INV-002 (AI proposes never executes) — L3 is the executor; AI agents will need to flow through it.
- INV-005 (evidence append-only) — L9.1 enforces this from the moment the log is open.
- INV-019, INV-020, INV-023 (visual identity, trust indicators always visible, CHROME zone reserved) — KDE renderer brings up chrome in compliance with the constitutional layer.

**Evidence record type.**

- `RUNTIME_INITIALIZED` (S10.1 §13 — STANDARD_24M).
- `EVIDENCE_LOG_OPENED` (S3.1 §13 — STANDARD_24M; canonical name in S3.1's Wave-N additions).
- `KDE_RENDERER_STARTED` (S7.4 §10 / S3.1 §24.1 — STANDARD_24M).

**Failure modes.**

- Capability Runtime fails to come up: actions cannot be submitted; operator has read-only observation surface only.
- Evidence Log degraded mode: see S3.1 §11.4 — appends paused until operator investigates; the golden path **cannot start** because step 16's evidence emission would fail.
- KDE renderer failure: operator has no visual surface; falls back to CLI renderer (S7.4 deferred to L7.4 — out-of-scope for KDE-renderer fixture but the surface composition spec at S7.1 admits CLI as a renderer kind).

**Acceptance check.** The step is complete iff: `RUNTIME_INITIALIZED`, `EVIDENCE_LOG_OPENED`, and `KDE_RENDERER_STARTED` evidence records are appended; the AIOS chrome zone is rendered above any other surface (INV-020); and the operator can launch the evidence viewer.

**Honesty.** Fully specified — S10.1 §13, S3.1 §13/§24, S7.4 §10 cover the bring-up. The Wave-N evidence record names are narrative-only in S3.1's Appendix A but contract-named in the §24/§25/§26 narrative sections.

### §4.7 Step 7 — Operator opens the KDE evidence viewer

**Layer + sub-spec + section.** L7 — S7.1 §3 (`SurfaceKind`); S7.4 §4 (`NodeKind` → Qt/QML compilation); S7.4 §11 Fixture 1 (the canonical KDE evidence viewer composition).

**Inputs.** Operator clicks the evidence-viewer icon in the system tray (S7.4 §6.2) or invokes via KRunner (S7.4 §6.1, §11 Fixture 5).

**Outputs.** A new `Surface` is created via `CreateSurface` (S7.1 §6.1, §11 RPC):

- `SurfaceKind = AIOS_SURFACE` (the viewer is rendered by AIOS UI schema, not by an external app)
- `CompositionZone = CONTENT` (S7.1 §4.1)
- `owner_subject_canonical_id = "family:alice"` (the operator)
- `namespace_path = "/aios/groups/family/users/alice/desktop/<viewer-state>"` (S4.1 §6 `USR_DESKTOP`)

The KDE renderer compiles the viewer's `NodeKind` tree per S7.4 §4: a `CARD` containing a `HEADING`, a `TABLE` bound to an S2.1 view of evidence chain rows, with optional `EVIDENCE_LINK` nodes for click-through. The surface enters `SurfaceLifecycle.ACTIVE` per S7.1 §5.

**Note on SurfaceKind for the viewer.** The user task contract describes the viewer as "AIOS_SURFACE in CONTENT zone" — the renderer **draws** it via Qt/QML, but the **owner** is `family:alice`'s session, not an installed app. This places it cleanly in `AIOS_SURFACE` per S7.1 §3.1 (the AIOS UI schema is the renderer; there is no external app rendering its own pixels). Earlier scoping language that called the viewer an `APP_SURFACE` was inaccurate; per S7.4 §11 Fixture 1, the evidence viewer is canonically an `AIOS_SURFACE`.

**INV citations.**

- INV-019 (visual identity preserved) — the viewer uses AIOS visual tokens, not Qt defaults.
- INV-020 (trust indicators visible) — the AIOS chrome remains on top in `wlr-layer-shell overlay`; the viewer is in CONTENT, below CHROME.
- INV-021 (AI vs human action visually distinct) — the viewer's rendering of evidence rows uses `COLOR_ACTION_AI` vs `COLOR_ACTION_HUMAN` tokens to distinguish (S7.4 §4 `AGENT_MESSAGE` cross-reference; in this scenario only HUMAN actions appear).
- INV-023 (CHROME zone reserved) — the viewer cannot author into CHROME; rejected at S7.1 §6.1 step 5 with `CompositionZoneForbidden`.

**Evidence record type.** `SURFACE_CREATED` (S7.1 §9 / S3.1 §24.1 — STANDARD_24M).

**Failure modes.**

- GPU capability denied: surface stays `DRAFT`; renderer falls back to CPU-only path or fails closed (S7.1 §6.1 step 6 + L8.2 deferred).
- Cross-group violation: not applicable here — operator is in their own group.
- Recovery-mode constraint: if `subject.recovery_mode = true`, the renderer rejects `APP_SURFACE`/`STREAM_SURFACE` per S7.1 I6; the evidence viewer is `AIOS_SURFACE` and is permitted in recovery as well (recovery shell is itself an `AIOS_SURFACE`).

**Acceptance check.** The step is complete iff a `Surface` record exists in `ACTIVE` state with kind `AIOS_SURFACE`, zone `CONTENT`, owner `family:alice`, and `SURFACE_CREATED` evidence is recorded.

**Honesty.** Fully specified — S7.1 §6 + S7.4 §11 Fixture 1 cover this completely.

**GAP-§Step.7 (genuine).** The user-task contract asks: "is the subscription mechanism specified or is it implicit?" The viewer subscribes to the L9.1 evidence stream to render new rows live. **Resolution:** S3.1 §9 (`Subscribe` RPC) **is specified**; the viewer holds an `EvidenceLog.Subscribe` stream filtered by `subject_filter = "family:alice"` or `correlation_id_filter` for follow-along on a specific action. Bookmarks (S3.1 §9.1) provide reconnect resume. **No gap.**

### §4.8 Step 8 — Operator types the goal; intent perception

**Layer + sub-spec + section.** L5 — S13.1 (Cognitive Core Model — intent perception path); S1.1 (Capability Translator entrypoint).

**Inputs.** The operator's natural-language utterance (Bulgarian or English): `"Add journal entry: I went hiking today"`. Entered into a goal-entry FORM tree of an AIOS_SURFACE rendered by S7.4 §11 Fixture 5 (KRunner integration) or an equivalent goal-entry surface.

**Outputs.** An `intent_id = intent_<ULID>` is created by L5 (S13.1) tagging the operator's goal. The intent record carries: subject = `family:alice`, raw utterance, language tag (BG / EN), `correlation_id = corr_<ULID>` (root of the trace; will propagate to the action envelope at step 10).

**INV citations.**

- INV-002 (AI proposes never executes) — the cognitive core is here only to **translate**; it does not execute. The actor on the action envelope will be `family:alice`, not the cognitive core.
- INV-007 (downward dependency) — L5 depends on L4 + L0 only; not on L7 or L9 for correctness. The renderer carries the utterance to L5; L5 doesn't depend on the renderer.

**Evidence record type.** `TRANSLATION_CREATED` (S3.1 §4 `RecordType` enum entry 2; STANDARD_24M).

**Failure modes.**

- L5 unavailable (model down, planner offline): the goal-entry form falls back to "submit as-is" mode; the operator can type a typed-action template directly. For the MVP scenario the L5 path is the canonical one.
- Language not understood: S1.1 §17 refusal codes; the operator gets a clarification request; no envelope is created.

**Acceptance check.** The step is complete iff an `intent_<ULID>` record exists tagged with the operator subject and the utterance, and `TRANSLATION_CREATED` evidence is appended.

**Honesty.** Partially specified at the cognitive-core boundary. S13.1 is `CONTRACT` per the master index. The **GAP-§Step.8 (minor)** the user-task contract names is: "is content embedding in target specified or is it ad-hoc?" — addressed in step 9.

### §4.9 Step 9 — Capability Translator compiles to typed `aios.fs.write` action

**Layer + sub-spec + section.** L5 — S1.1 (Capability Translator). The translator consults the L5 capability catalog at `/aios/system/capabilities/` (S4.1 §4 `SYS_CAPABILITIES`).

**Inputs.** The intent record from step 8; the active capability catalog (`catalog_version = cat_<hex>` per S1.1 §D1).

**Outputs.** A typed action proposal:

- `action = "aios.fs.write"`
- `target.path = "/aios/groups/family/users/alice/journal/2026-05-11.md"` (resolved via S4.1 §8.2 from the operator's primary group, user, and the journal subfolder)
- `target.scope = USER`, `target.group_id = "family"`, `target.user_id = "alice"`
- `target.content = "I went hiking today.\n"` (the literal bytes from the operator's utterance after normalization)
- `target.create_if_missing = true` (the file doesn't yet exist; the AIOS-FS adapter will create the object + first version)
- `verification = [{ aiosfs.pointer, object_id="<resolved>", pointer_kind="CURRENT", expected_version_id="<staged>" }]` (S2.4 §4.1)
- `subject = "family:alice"`
- `reason = "daily journal entry"` (S0.1 §4.5)
- `risk = { all flags false }` (per §3.3)
- `idempotency_key = <translator-stable hash of (intent_id, action, target.path, target.content)>` (S0.1 §3.3)

**INV citations.**

- INV-002 (AI proposes never executes) — even though L5 is involved in **translation**, the actor on the envelope is the operator. The translator does not submit; the renderer's submit affordance does, on the operator's session credential.
- INV-007 (downward dependency) — S1.1 depends on the catalog (system scope) and the operator's identity, both at or below L5.

**Evidence record type.** Covered by `TRANSLATION_CREATED` (already emitted at step 8); the translator may emit a finer-grained `TRANSLATION_RESOLVED` if its narrative names one — narrative-only at present.

**Failure modes.**

- No catalog match: S1.1 §17 refusal code `NoCatalogMatch`; the operator gets a clarification request.
- Ambiguity (multiple matches): S1.1 §17 `Ambiguity`; clarification request.
- Underspecified high-risk action: S1.1 §17 `RequireMoreContext`. Not applicable to this scenario (low-risk).
- Adversarial input (prompt injection in the utterance, secret-shaped content): S1.1 §D2 adversarial defenses — `reason` is sanitized, `target` is schema-validated, embedded shell is rejected, secret-shaped content is redacted. For the journal entry case, the content is benign text and the path resolves to operator-private namespace.

**Acceptance check.** The step is complete iff a typed action proposal exists matching the schema for `aios.fs.write` (validated against the adapter manifest's `target_schema` per S10.1 §10.1) and the proposal is ready to be wrapped into an envelope at step 10.

**Honesty.** Fully specified — S1.1 §D1..D9 cover the translation surface; the adapter manifest schema is owned by S10.1 §10.

**GAP-§Step.9 (genuine, addressed).** The user-task contract asks: "is content embedding in target specified or is it ad-hoc?" **Resolution:** S0.1 §4.3 (`request.target` is `google.protobuf.Struct`, schema per the adapter manifest) and S10.1 §10.1 (`AdapterActionDeclaration.target_schema`) jointly specify that the per-action target schema is owned by the adapter manifest. The `aios.fs.write` adapter declares whether content is embedded in `target.content` (small files) or referenced by `target.content_chunk_ref` (large files via prior chunk upload). For the MVP scenario the journal entry is small (≤1 KB) and embedded inline. **No spec gap; the resolution is a per-adapter manifest detail owned by the AIOS-FS adapter; the structural contract is in place.**

### §4.10 Step 10 — ActionEnvelope built; identity & request immutable

**Layer + sub-spec + section.** S0.1 §2 (envelope structure); §3 (identity); §4 (request).

**Inputs.** The typed action proposal from step 9; the operator's signed `Subject` record from step 5.

**Outputs.** A complete `ActionEnvelope`:

- `identity.action_id = act_<ULID>`
- `identity.idempotency_key = <translator-stable hash>` (per S0.1 §3.3)
- `identity.created_at = <wall clock>`
- `identity.intent_id = intent_<ULID>` (from step 8)
- `identity.correlation_id = corr_<ULID>` (root of the trace)
- `request` populated per §3.3
- `request_hash = hex_lower(BLAKE3(JCS(request)))[:32]` per S0.1 §8.5
- `execution.phase = PENDING_UNSPECIFIED` (will become `PENDING` at runtime intake at step 11)
- `trace.trace_id = <W3C trace context>` (from the renderer's gRPC interceptor per S0.1 §9.1)

The envelope is sent via `SubmitAction` to the L3 Capability Runtime ingress (S0.1 §10.2).

**INV citations.**

- INV-002 (AI proposes never executes) — `request.subject = "family:alice"` is the operator (HUMAN_USER); not an AI subject.
- INV-014 (no proof, no completion) — the envelope is the seed of the proof chain; nothing claims completion until verification + evidence at steps 15–16.

**Evidence record type.** No record at this step; records start at step 11 when the runtime accepts the envelope.

**Failure modes.**

- Envelope schema invalid: `gRPC InvalidArgument`; no envelope ever exists in the runtime.
- Subject signature invalid: rejected at the public ingress per S10.1 §12.10 (`Subject-cert binding mismatch never creates an L3 lifecycle`); FOREVER evidence.
- Idempotency conflict (same `idempotency_key` + different `hash(request)` within TTL): rejected with `IdempotencyConflict` per S0.1 §3.3 table row 2.

**Acceptance check.** The step is complete iff a well-formed `ActionEnvelope` reaches the L3 ingress with valid subject signature and well-formed `request_hash`.

**Honesty.** Fully specified — S0.1 §2..§4, §8 cover this completely.

### §4.11 Step 11 — `ValidateAction` (L3 internal RPC #1 of 9)

**Layer + sub-spec + section.** L3 — S10.1 §5.1 (gRPC service definition); S10.1 §6.1 step 0 (validation) — note S10.1's eight-step sequence is for `ExecuteAction`; `ValidateAction` is the **first** RPC in the nine-RPC orchestration model that the public ingress walks. The lifecycle transition is `(init) → CREATED` per S10.1 §3.1.

**Inputs.** The envelope from step 10.

**Outputs.** Validation results:

- Schema validation (envelope conforms to `aios.action.v1alpha1`).
- Target validation (`target` matches `aios.fs.write` adapter's `target_schema` from the adapter manifest at `/aios/system/capabilities/` — referenced by S1.1 catalog and S10.1 §10).
- Sandbox profile composition test: `SandboxComposer.ComposeProfile` (S3.2 §3) produces a candidate profile for `(adapter, action_kind, subject)` — the result will include `RUNTIME_LINUX_NATIVE` because `aios.fs.write` is a native AIOS-FS operation.
- Verification grammar validation: each `VerificationIntent` is checked against the closed primitive vocabulary (S2.4 §4.2).
- Namespace validation: `target.path` is resolved by S4.1 §8.2; the resolution emits a typed `NamespacePath` matching `target.scope/group_id/user_id/user_reserved`.

The lifecycle is set to `CREATED`.

**INV citations.**

- INV-007 (downward dependency) — the validator only consults L0 + L2 + L3 + L4 (capability catalog) + L9 (evidence); no L5 dependency.
- INV-014 (no proof, no completion) — failed validation never reaches policy.

**Evidence record types.**

- `ACTION_RECEIVED` (S3.1 §4 entry 1; STANDARD_24M)
- `ACTION_VALIDATED` (S10.1 §13 / S3.1 §25.5; STANDARD_24M)

**Failure modes.**

- Schema validation fails: lifecycle → `FAILED` with `ExecutionFailureReason = ENVELOPE_VALIDATION_FAILED` (S10.1 §3.6); evidence: `ACTION_VALIDATED` with `result = FAILED`.
- Target schema fails: identical fail-closed.
- Sandbox composition fails (no profile possible under the floor): lifecycle → `FAILED` with `SANDBOX_APPLICATION_FAILED`.
- Verification grammar invalid (unknown primitive, malformed args): rejected at submission per S2.4 §4.3 with `INVALID_INTENT`.
- Namespace resolution fails (path traversal, invalid reserved name, segment count exceeded): S4.1 §8.1 `ResolutionError`; lifecycle → `FAILED` with `ENVELOPE_VALIDATION_FAILED`.

**Acceptance check.** The step is complete iff the envelope is in `CREATED` state and `ACTION_VALIDATED` evidence is appended (or `FAILED` with appropriate evidence).

**Honesty.** Fully specified — S10.1 §5, §6.1, §10 cover validation; S2.4 §4 covers verification grammar; S4.1 §8 covers namespace resolution.

### §4.12 Step 12 — `EvaluatePolicy` (L4.1 Policy Kernel)

**Layer + sub-spec + section.** L4 — S2.3 (Policy Kernel); decision pipeline at S2.3 §3; result schema at S2.3 §4; rule precedence at S2.3 §5.

**Inputs.** The validated envelope from step 11.

**Outputs.** A `PolicyDecision`:

- `policy_decision_id = poldec_<ULID>`
- `action_id = act_<ULID>`
- `request_hash = <from envelope>`
- `bundle_version = polb_<hex>`
- `enrichment_snapshot_id = <S1.3 SNAPSHOT id>` (resource enrichment per S2.3 §8 read the (yet-to-be-created) target object; for create-if-missing the enrichment confirms the parent path resolves)
- `decision = ALLOW` (the journal write is in the operator's own private namespace, group `family`, user `alice`; the bundle's family group rule allows owner-private writes without approval at low risk)
- `reason_code = "ScopedAllow"` (S2.3 §4 canonical short code)
- `constraints = { applied_sandbox_profile_id_floor: "default_user_floor" }` (S2.3 §10)

The lifecycle transitions: `CREATED → POLICY_PENDING → APPROVED` (S10.1 §3.1) — the action skips `APPROVAL_PENDING` because `decision = ALLOW`, not `REQUIRE_APPROVAL`.

**INV citations.**

- INV-008 (default deny in policy) — without an explicit allow rule, the decision would be DENY. The bundle has an explicit allow for owner-private writes.
- INV-010 (AI cannot self-approve) — not engaged because subject is HUMAN_USER.
- INV-011 (cross-group access forbidden) — not engaged because target group = subject's primary group.
- INV-013 (AI cannot system admin) — not engaged because subject is not AI.
- S2.3 hard-deny `RecoveryRequiredForSystemMutation` — not triggered because target is not in `RecoveryMutableScope` (it's under `groups/family/users/alice/journal/`, not under `system/`).

**Evidence record types.**

- `POLICY_DECISION` (S3.1 §4 entry 4; STANDARD_24M for ALLOW; FOREVER for DENY/REQUIRE_APPROVAL per S3.1 §13).
- `ACTION_POLICY_DECISION` (S10.1 §13 — the runtime's mirror; STANDARD_24M).

**Failure modes.**

- DENY: lifecycle → `POLICY_DENIED` (terminal); evidence at FOREVER. Operator sees a refusal in the renderer.
- REQUIRE_APPROVAL: lifecycle → `APPROVAL_PENDING`; covered by step in §7 scenario A.
- Bundle signature failure at load (separate event, not at evaluation time): policy kernel enters degraded mode (only hard-denies + emergency override active per S2.3 §12.4); every action denied.
- Enrichment fails (object missing, AIOS-FS degraded): short-circuit to DENY with `EnrichmentUnavailable` per S2.3 §8.
- Evaluation timeout / engine error: short-circuit to DENY with `PolicyEvaluationTimeout` / `PolicyEngineInternal` per S2.3 §18.2 (fail-closed).

**Acceptance check.** The step is complete iff a `PolicyDecision` record with `decision = ALLOW`, `reason_code = ScopedAllow`, and a non-empty `enrichment_snapshot_id` exists, and the action lifecycle is in `APPROVED`.

**Honesty.** Fully specified — S2.3 §3..§13 cover the decision pipeline; S10.1 §3.1 covers the lifecycle transitions.

### §4.13 Step 13 — `ExecuteAction` — eight-step pre-dispatch

**Layer + sub-spec + section.** L3 — S10.1 §6 (eight-step pre-dispatch sequence); §3.2 (`ActionDispatchKind` decision); §6.2 (dispatch envelope).

**Inputs.** The approved envelope from step 12; the active policy bundle version; the active sandbox composer.

**Outputs.** The L3 runtime executes the eight steps of S10.1 §6.1 in strict order:

1. **Re-validate canonical hash.** No binding required for ALLOW (no approval was issued); step is a no-op for non-bound actions. (For approval-bound actions in §7.A, this step compares against the binding hash.)
2. **Re-evaluate policy decision against current bundle.** Bundle version unchanged → re-evaluation skipped.
3. **Re-check approval binding.** Not applicable (no approval).
4. **Re-check vault capability binding.** Not applicable (no vault capability required).
5. **Re-evaluate sandbox profile composition.** `SandboxComposer.ComposeProfile` produces `applied_sandbox_profile_id = "sb:fs_write_user_private"` per S3.2 §3 with composition source ordering `[adapter_default, app_manifest, user_request, policy_required, group_floor, runtime_safety_floor]` (S4.1 §12.7 / S3.2 §5.1) and `ecosystem_runtime = RUNTIME_LINUX_NATIVE`.
6. **Re-check action lifecycle is `QUEUED`.** Action is in `QUEUED` after a `APPROVED → QUEUED` transition (queue class = `INTERACTIVE` because subject is human and environment is `LOCAL`). Lifecycle in `QUEUED` ✓.
7. **Mark binding `CONSUMED`.** Not applicable.
8. **Transition lifecycle to `EXECUTING` and dispatch.** `ActionDispatchKind` decision per S10.1 §3.2: subject is human, `risk.privileged = false`, adapter manifest declares `SUBPROCESS_FORK`, adapter stability is `STABLE` → dispatched as `SUBPROCESS_FORK` (per the §3.2 decision rule fallback). The adapter `adapter:aios:fs:1.0.0` is invoked with the typed `target` and the composed sandbox profile.

The lifecycle transitions: `APPROVED → QUEUED → EXECUTING`.

**INV citations.**

- INV-014 (no proof, no completion) — every step is run; skipping any step is a constitutional violation per S10.1 §6.1.
- INV-002 — re-asserted: dispatch goes through the runtime, not directly from the user.
- INV-007 — dispatch only consumes L0..L3 + L4 (binding state) + L6 (sandbox composition).

**Evidence record types.**

- `ACTION_DISPATCHED` (S10.1 §13 / S3.1 §25.5; STANDARD_24M).
- `SANDBOX_PROFILE_APPLIED` (queued in S3.1 — exact name in S3.2's evidence vocabulary; STANDARD_24M).

**Failure modes.**

- Step 1 hash mismatch (when binding-bound): `BINDING_VOIDED_ACTION_REVISED` (FOREVER); lifecycle → `FAILED`. Not applicable here.
- Step 2 bundle drift: `BINDING_EXPIRED`; lifecycle → `FAILED`.
- Step 5 sandbox composition fails: `SANDBOX_APPLICATION_FAILED`; lifecycle → `FAILED`.
- Step 6 lifecycle not in `QUEUED` (race with cancel/rollback): `LIFECYCLE_ILLEGAL_TRANSITION`; abort.
- Step 8 adapter unavailable: `BACKEND_UNAVAILABLE` or `DEPENDENCY_UNREADY`; lifecycle → `FAILED`.

**Acceptance check.** The step is complete iff all eight pre-dispatch steps pass, the action lifecycle is `EXECUTING`, the adapter is invoked with a valid `AdapterDispatchEnvelope` (S10.1 §6.2), and `ACTION_DISPATCHED` evidence is appended.

**Honesty.** Fully specified — S10.1 §6 covers the eight-step sequence in detail.

### §4.14 Step 14 — AIOS-FS object created (chunk write + version + pointer)

**Layer + sub-spec + section.** L2 — S1.3 §6 (Version record); §7 (chunk model); §8 (pointer record + CAS protocol); §9 (transaction model). Adapter operates against `aios.fs.v1alpha1.AIOSFSObjects` per S1.3 §D2.

**Inputs.** The dispatch envelope from step 13 with `target.path`, `target.content`, `subject = family:alice`, `applied_sandbox_profile_id = sb:fs_write_user_private`.

**Outputs.** The AIOS-FS adapter executes a single transaction (S1.3 §9):

1. `BeginTransaction(subject="family:alice", action_id=act_<ULID>)` → `transaction_id = txn_<ULID>` in `PENDING_TX`.
2. The adapter creates the object record per S1.3 §5: `obj_<ULID>`, `kind = FILE`, `created_by = family:alice`, `privacy_class = SENSITIVE`, `policy_tags = ["personal", "journal"]`, `lifecycle_state = ACTIVE`, `scope_binding = (USER, family, alice)` per S1.3 §D2 / S4.1 §12.2.
3. The content `"I went hiking today.\n"` is chunked per S1.3 §7.2 (FastCDC default; for ≤1 KB content, a single chunk). The chunk gets `chunk_id = chk_<full BLAKE3-256 hex (64 chars, no truncation per S1.3 §3.2)>`, `size_bytes ≈ 22`, `ref_count = 1`, `created_at = now`.
4. `WriteVersion(object_id=obj_<ULID>, parent_version_ids=[], chunk_refs=[chk_<...>], ...)` → `ver_<ULID>` in `STAGED` state with `created_by_action_id = act_<ULID>`, `created_by_transaction_id = txn_<ULID>`.
5. `PromotePointer(pointer_id=ptr_<ULID>, expected_current_version_id="" (new pointer), new_version_id=ver_<ULID>)` — atomic CAS per S1.3 §8.2. For a new pointer the expectation is empty/null; the CAS succeeds and `last_promoted_at = now`, `last_promoted_by_transaction_id = txn_<ULID>`.
6. `CommitTransaction()` → `transaction_state = COMMITTED`; `chunk_ref_count_increment: { chk_<...>: 1 }`; the version's state moves from `STAGED` to `VERIFIED` after step 15 verification.

The path `/aios/groups/family/users/alice/journal/2026-05-11.md` resolves through S4.1 §8.2 to the typed `NamespacePath{ scope=USER, group_id="family", user_id="alice", user_reserved=USR_HOME (or a journal subfolder under the user's home), subpath=["journal", "2026-05-11.md"], is_virtual_view=false, namespace_catalog_version="nscat_<hex>" }`.

**INV citations.**

- INV-004 (recovery boundary preserved) — the write goes to `/aios/groups/...`, not to `/` or `/root`.
- INV-005 (evidence append-only) — the AIOS-FS transaction commits an evidence receipt linked at S1.3 §16 / S0.1 §5.6.
- INV-011 (cross-group access forbidden) — subject group `family` matches target group `family`; not engaged.
- S1.3 §D5 (privacy class monotonic) — class can only be raised; this is a fresh object so the constraint is satisfied trivially.

**Evidence record types.**

- `EXECUTION_STARTED` (S3.1 §4 entry 8; STANDARD_24M).
- `AIOSFS_TRANSACTION_COMMITTED` (queued in S3.1 — the L2 evidence vocabulary names this; STANDARD_24M).
- The transaction record's `evidence_receipt_id` field links to one of these receipts per S1.3 §9.1.

**Failure modes.**

- CAS conflict: not applicable for a fresh pointer; for concurrent writes, S1.3 §8.2 returns `ConflictDetected`.
- Chunk write failure: transaction aborts; chunk becomes orphan-eligible after 24h staging TTL per S1.3 §7.3 — covered as `GAP-§Step.14` resolution below.
- Quarantine on integrity failure: S1.3 §12 (chunk hash mismatch on read) — not applicable on write; the adapter writes content-addressed bytes whose hash is computed from the bytes themselves.
- Multi-pointer transaction CAS failure: not applicable (single-pointer transaction); for multi-pointer, S1.3 §8.3 atomicity rule.

**Acceptance check.** The step is complete iff a `Transaction` record with `state = COMMITTED` exists, the new `ver_<ULID>` is referenced by `ptr_<ULID>` (kind = `CURRENT`), the chunk `chk_<...>` has `ref_count = 1`, and the AIOS-FS evidence receipt is appended.

**Honesty.** Fully specified — S1.3 §5..§13 + §D1..D7 cover the write path.

**GAP-§Step.14 (genuine, addressed).** The user-task contract asks: "is the atomicity guarantee fully specified or are there race windows?" **Resolution:** S1.3 §8.2 (CAS atomicity for single pointer) + §8.3 (two-phase commit fence for multi-pointer transactions within one AIOS-FS instance) + §9.2 (transaction lifecycle: `PENDING_TX → COMMITTING → COMMITTED | ABORTED`) **fully specify** the atomicity guarantee for single-instance writes. Cross-instance multi-object transactions are out of scope per S1.3 §17 — not relevant for the MVP. Orphan-staging cleanup at 24h TTL (S1.3 §7.3) covers the partial-write recovery case. **No race window for single-instance MVP.** The audit map records this resolution under §10 dependency map. See also §11.3 (worked failure trace for orphan-chunk GC).

### §4.15 Step 15 — `VerifyAction` — verification engine runs

**Layer + sub-spec + section.** L9 — S2.4 (Verification Grammar); §4.1 (`aiosfs.pointer` primitive); S10.1 §7.1 (verification handoff).

**Inputs.** The completed transaction from step 14; the envelope's `request.verification` array (one `aiosfs.pointer` intent).

**Outputs.** The verification engine runs the `aiosfs.pointer` primitive per S2.4 §4.1:

- `args.object_id = obj_<ULID>` (resolved at validation time from the path)
- `args.pointer_kind = "CURRENT"`
- `args.expected_version_id = ver_<ULID>` (the version the adapter just promoted)
- The probe queries AIOS-FS and observes `ptr_<ULID>.current_version_id = ver_<ULID>` — match.

`VerificationResult.status = VERIFICATION_PASSED`; observed = `{ observed_version_id = ver_<ULID>, last_promoted_at = <ts> }`.

The lifecycle transitions: `EXECUTING → VERIFYING → SUCCEEDED` per S10.1 §3.1 + §7.1.

**INV citations.**

- INV-014 (no proof, no completion) — verification is the proof; without it, the action is not `SUCCEEDED`.
- L0.4 §3 INV-005 — the verification probe is read-only per S2.4 §6.1; it cannot modify the evidence log or the AIOS-FS state.

**Evidence record types.**

- `VERIFICATION_RESULT` (S3.1 §4 entry 10; default 180 days per S3.1 §13).
- `EXECUTION_SUCCEEDED` (S10.1 §13; STANDARD_24M).
- The `STATUS_TRANSITION` carries the `SUCCEEDED` phase change; the receipt links to the action_id.

**Failure modes.**

- `VERIFICATION_FAILED` (pointer doesn't match): lifecycle → `FAILED`; if the adapter declared `rollback_strategy != NONE`, S10.1 §7.2 attempts rollback; for `aios.fs.write` the typical strategy is `IDEMPOTENT_REVERSE` (recompute prior pointer state and CAS back) or `CHECKPOINT_BASED`.
- `VERIFICATION_TIMEOUT`: lifecycle → `FAILED` per S10.1 §7.1.
- `VERIFICATION_SKIPPED` (only valid in `VALIDATE` mode): treated as failure outside `VALIDATE` per S10.1 §7.1.
- Verification primitive fails closed: `VERIFICATION_PROBE_ERROR` per S2.4 §6 → lifecycle `FAILED`.

**Acceptance check.** The step is complete iff `VerificationResult.status = VERIFICATION_PASSED` for the single intent, the action lifecycle is `SUCCEEDED`, and `EXECUTION_SUCCEEDED` evidence is appended.

**Honesty.** Fully specified — S2.4 §4.1 covers the `aiosfs.pointer` primitive; S10.1 §7.1 covers the verification handoff.

### §4.16 Step 16 — Evidence chain emitted; signed DAG formed

**Layer + sub-spec + section.** L9 — S3.1 §3 (receipt shape); §5 (hash chain algorithm); §13 (retention policy); §11 (adversarial robustness).

**Inputs.** The receipts emitted across steps 11–15.

**Outputs.** The full evidence chain for this action, all linked via:

- `correlation_id = corr_<ULID>` (set at envelope creation; carried through every receipt per S3.1 §3 receipt schema).
- `previous_receipt_hash` — each receipt's hash chains to the previous segment-local receipt per S3.1 §5.1.
- `parent_receipt_id` (queued in S3.1 — implicit through `correlation_id` + `action_id` linkage; explicit DAG edges via the hash chain algorithm per S3.1 §5.1, §5.2).

The chain in chronological order:

```text
ACTION_RECEIVED                 (step 11, evr_A — STANDARD_24M)
ACTION_VALIDATED                (step 11, evr_B — STANDARD_24M)
POLICY_DECISION                 (step 12, evr_C — STANDARD_24M for ALLOW)
ACTION_POLICY_DECISION          (step 12, evr_D — STANDARD_24M; runtime mirror)
SANDBOX_PROFILE_APPLIED         (step 13, evr_E — STANDARD_24M)
ACTION_DISPATCHED               (step 13, evr_F — STANDARD_24M)
EXECUTION_STARTED               (step 14, evr_G — STANDARD_24M)
AIOSFS_TRANSACTION_COMMITTED    (step 14, evr_H — STANDARD_24M)
VERIFICATION_RESULT             (step 15, evr_I — 180d)
EXECUTION_SUCCEEDED             (step 15, evr_J — STANDARD_24M)
STATUS_TRANSITION SUCCEEDED     (step 15, evr_K — STANDARD_24M)
```

All eleven receipts share `correlation_id = corr_<ULID>` and `action_id = act_<ULID>`. Each receipt's `previous_receipt_hash` chains to the previous receipt within its segment per S3.1 §5.1; cross-segment linkage is the `CHAIN_CHECKPOINT` mechanism per S3.1 §5.2. The chain is signed at the segment level (Ed25519 per S3.1 §11.3); the sealed segments form a tamper-evident DAG when traversed via `Subscribe` (S3.1 §9) or `Query` (S3.1 §10).

**INV citations.**

- INV-005 (evidence append-only) — every receipt is appended; none is mutated.
- INV-014 (no proof, no completion) — every transition has a receipt.
- INV-015 (evidence never contains secrets) — none of the receipts contain raw secret material; the journal text is `SENSITIVE` but is a benign user message, not secret material per S1.2 / S1.3 §4.1 distinction.

**Evidence record types.** All listed above. Each is a `RecordType` enum entry per S3.1 §4 + Wave-N additions (post-Wave 7 the narrative total is **205 entries**; every receipt above maps to a contract-named entry).

**Failure modes.**

- Hash chain inconsistency detected: `CHAIN_INCONSISTENCY_DETECTED` per S3.1 §11.4; engine enters degraded mode.
- Tamper detected: `TAMPER_DETECTED` per S3.1 §11.5; engine degraded; operator alert.
- Append-replay attack: `SEQUENCE_REPLAY_DETECTED` per S3.1 §11.1.

**Acceptance check.** The step is complete iff:

- All eleven receipts above (or a superset including the canonical record types) exist with the same `correlation_id`.
- `VerifyChain` (S3.1 §17) returns `chain_valid = true` for the segments containing these receipts.
- The chain forms a single signed DAG anchored at `ACTION_RECEIVED`.

**Honesty.** Fully specified — S3.1 §3, §5, §11, §13, §17 cover the chain algorithm and verification.

### §4.17 Step 17 — KDE renderer surface updates

**Layer + sub-spec + section.** L7 — S3.1 §9 (`Subscribe` RPC); S7.1 §6.2 (composition); S7.4 §11 Fixture 1 (KDE evidence-viewer composition).

**Inputs.** The viewer surface from step 7 holds an active `EvidenceLog.Subscribe` stream (S3.1 §9) filtered by the operator's subject and/or by the `correlation_id` of the active action (subscribed at step 10 when the envelope was submitted).

**Outputs.** The viewer surface receives the receipts from step 16 in chronological order via the `Subscribe` stream. The renderer:

- Translates the receipt rows into a `LIST` / `TABLE` model per S7.4 §4 row mapping.
- Applies `INV-021` distinct visual treatment: `COLOR_ACTION_HUMAN` token (this action is HUMAN-authored).
- Renders an `EVIDENCE_LINK` node next to the action's row, linking to a deeper detail surface that opens on click.
- Updates the surface's CONTENT zone via the composition pipeline (S7.1 §6.2). The CHROME zone (above) shows the operator's identity badge and the current action context per INV-020.

The result: the operator visually sees the new journal entry appear in the evidence viewer, with the action marked `SUCCEEDED`, the verification badge green, and a click-through to the full chain.

**INV citations.**

- INV-019 (visual identity preserved) — AIOS visual tokens applied; not Qt defaults.
- INV-020 (trust indicators visible) — chrome remains on top; subject + action_id visible.
- INV-021 (AI vs human visual distinct) — HUMAN treatment applied.
- INV-023 (CHROME zone reserved) — CHROME is owned by the renderer's `aios_chrome` system identity; the viewer cannot author there.

**Evidence record types.** Renderer-side observations are not evidence-emitting per se — the renderer **consumes** evidence. However, surface lifecycle events from step 7 (`SURFACE_CREATED`) and any user interaction with the surface that triggers further actions (e.g., clicking `EVIDENCE_LINK` to open detail) emit their own evidence per S7.1 §9.

**Failure modes.**

- Subscribe stream backpressure: per S3.1 §9.3, slow consumers get a `subscriber_dropped_event` and may miss receipts; on reconnect the bookmark mechanism (S3.1 §9.1) replays.
- Renderer crash: KDE renderer restarts; surface is re-created; subscribe resumes from bookmark.
- Theme token resolution failure: per S7.4 §4.3 `KDE_THEME_TOKEN_UNRESOLVED`; renderer falls back to AIOS default theme; viewer remains visible.

**Acceptance check.** The step is complete iff:

- The viewer's CONTENT-zone `AIOS_SURFACE` shows the new journal entry row.
- The action's `STATUS_TRANSITION SUCCEEDED` row is visible.
- The CHROME zone above the viewer remains unobstructed (INV-020).
- The visual treatment is `COLOR_ACTION_HUMAN` (INV-021).

**Honesty.** Fully specified — S3.1 §9 (Subscribe), S7.1 §6 (composition), S7.4 §4 + §11 Fixture 1 cover the rendering. The user-task contract's GAP-§Step.7 question about subscription is resolved here.

**GAP-§Step.17 — none. The step produces a renderer-side artifact, not just an evidence record. The operator visually confirms — that's the artifact.**

### §4.18 Step 18 — Operator visually confirms; trace complete

**Layer + sub-spec + section.** L7 — S7.4 (KDE renderer); the operator is the human-in-the-loop.

**Inputs.** The operator sees the rendered viewer surface from step 17.

**Outputs.** Operator confirms visually that the journal entry is recorded. No further system action is required; the trace is complete. The renderer surface remains active until the operator dismisses it.

**INV citations.**

- INV-014 (no proof, no completion) — the operator's visual confirmation is the human-in-the-loop closure of the proof. The system's evidence chain is the machine-checkable artifact; the operator's confirmation is the human-checkable artifact.

**Evidence record types.** None — the operator's confirmation is not a system event. If the operator dismisses the surface, a `SURFACE_DESTROYED` evidence is emitted per S7.1 §9 (STANDARD_24M).

**Failure modes.**

- Operator does not see expected entry: indicates a discrepancy between the action chain and the renderer output. The operator should investigate via `VerifyChain` (S3.1 §17) or by querying the evidence log directly. This is a real-world MVP test failure, not a spec failure.

**Acceptance check.** The step is complete iff the operator's experience matches the system's recorded state: the action is `SUCCEEDED`, the journal entry exists at the expected path, the verification passed, and the evidence chain is complete and signature-valid.

**Honesty.** Fully specified at the structural level. Operator UX details are owned by S7.4 §11 fixtures and the visual language spec S7.3.

## §5 Acceptance criteria for "MVP golden path achievable"

The binary checklist below is the contract-grade definition of "MVP golden path achievable". The claim "the AIOS rev.2 specification is implementable" is supported iff every box is ticked.

- [x] **Every step in §4 maps to a CONTRACT-grade or REAL sub-spec section.** The 18 steps are sourced from S9.1, S5.1, S2.2, S1.3, S4.1, S2.3, S0.1, S10.1, S2.4, S3.1, S7.1, S7.4 — all `CONTRACT` or `REAL` per the master index.
- [x] **Every step's INV citations are present in their cited specs.** INV-001..INV-024 are catalogued in L0.4 §3; every citation in §4 names an existing invariant.
- [x] **Every step's evidence record type exists in S3.1's RecordType narrative.** The narrative total is 205 entries (post-Wave 7); every record type named in §4 is in the closed enum or queued narratively in §24..§26 of S3.1.
- [ ] **Every step's failure mode maps to a closed FailureClass in L9.3 (S14.1).** **GAP-§Acceptance.4 (genuine):** L9.3 (admin operations / failure classes) is not yet a contract-grade sub-spec; the failure-mode language in §4 cites per-spec failure surfaces (e.g., S10.1 §3.6 `ExecutionFailureReason`, S2.3 §18.2, S4.1 §8.1 `ResolutionErrorCode`) which are closed in their owning specs but are not consolidated into a single L9.3 closed `FailureClass` enum. **Audit-finding placeholder:** queued for an L9.3 sub-spec (admin operations + failure-class consolidation).
- [x] **Every step's adversarial concern is enumerated in the source spec.** S10.1 §12, S2.3 §19, S1.1 §D2, S3.1 §11, S7.1 §I3-I8, S4.1 §I8 each carry adversarial-robustness sections.
- [x] **Step 17 produces a renderer-side artifact (not just an evidence record).** The KDE evidence viewer's CONTENT-zone `AIOS_SURFACE` shows the journal entry row visibly (S7.4 §11 Fixture 1).
- [x] **Total evidence chain from step 7 to 17 forms a single signed DAG.** Anchored on `correlation_id` (S0.1 §3.5) and chained via `previous_receipt_hash` (S3.1 §5.1); segment-signed (S3.1 §11.3); validated by `VerifyChain` (S3.1 §17). DAG discipline per S3.1 §5.

**Checkbox count: 6 of 7 ticked; 1 GAP queued (L9.3 FailureClass consolidation).**

The single gap is a **consolidation gap, not an implementability gap**: every individual failure surface is contract-grade in its owning spec; what's missing is a single L9.3 cross-cutting enum that lets the operator query "is this a $closed-failure-class?" uniformly. The MVP can be built without L9.3; the gap is queued for the next refinement cycle.

## §6 Honesty gates per step

Per the user-task directive ("be honest; if a gap is found, name it as a future-audit-finding placeholder"). The table below summarizes each step's specification status and any genuine gaps.

| Step | Honesty status      | Gap (if any)                                                                                                                                                                                                            |
| ---- | ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1    | Fully specified     | None.                                                                                                                                                                                                                   |
| 2    | Fully specified     | None.                                                                                                                                                                                                                   |
| 3    | Partially specified | **GAP-§Step.3 (minor):** S9.2 first-boot installer / NORMAL boot stage FSM is `DEFERRED`. Recovery analogue is fully specified in S9.1; NORMAL analogue is implied by symmetry but not contract-grade. Queued for S9.2. |
| 4    | Partially specified | **GAP-§Step.4 (minor):** boot-time evidence record name for AIOS-FS mount is queued in S3.1 but not yet contract-named; tracked under S9.2 deferral.                                                                    |
| 5    | Partially specified | The S5.1 evidence-record vocabulary for authentication is queued for S5.1 refinement; structural contract is in place. Not a blocker.                                                                                   |
| 6    | Fully specified     | None — Wave-N record names are narrative-only in S3.1 Appendix A but contract-named in §24/§25/§26.                                                                                                                     |
| 7    | Fully specified     | None. Subscription mechanism is S3.1 §9 (`Subscribe` RPC).                                                                                                                                                              |
| 8    | Partially specified | S13.1 cognitive core model is `CONTRACT`; the intent-record evidence vocabulary is implied via `TRANSLATION_CREATED` (S3.1 entry 2). Adequate for MVP.                                                                  |
| 9    | Fully specified     | None. Content embedding in `target` is per-adapter manifest (S10.1 §10.1 `target_schema` ownership); for `aios.fs.write` the manifest decides inline vs chunk-ref.                                                      |
| 10   | Fully specified     | None.                                                                                                                                                                                                                   |
| 11   | Fully specified     | None.                                                                                                                                                                                                                   |
| 12   | Fully specified     | None.                                                                                                                                                                                                                   |
| 13   | Fully specified     | None.                                                                                                                                                                                                                   |
| 14   | Fully specified     | None. Atomicity guarantee for single-instance writes is S1.3 §8.2 (CAS) + §8.3 (multi-pointer 2PC). Cross-instance is out of scope for MVP.                                                                             |
| 15   | Fully specified     | None.                                                                                                                                                                                                                   |
| 16   | Fully specified     | None. The DAG forms via `correlation_id` + segment-local `previous_receipt_hash` chain + `CHAIN_CHECKPOINT` cross-segment linkage.                                                                                      |
| 17   | Fully specified     | None.                                                                                                                                                                                                                   |
| 18   | Fully specified     | None at structural level; UX details are owned by S7.3 + S7.4.                                                                                                                                                          |

**Genuine gaps surfaced:** 2 minor (Step 3, Step 4) — both attributable to the same S9.2 deferral. The S9.2 first-boot installer / NORMAL boot stage FSM is the single missing piece; everything else in the trace resolves cleanly to existing CONTRACT or REAL sub-specs.

**Acceptance gap (§5):** 1 (L9.3 FailureClass consolidation) — non-blocking for MVP build; tracked for next refinement cycle.

## §7 Three additional traced scenarios

The canonical trace in §4 covers operator-driven low-risk write. The three scenarios below show the spec covers other paths.

### §7.1 Scenario A — Operator approves an AI agent's proposal

**Setup.** AI agent `family:family-assistant` (subject kind `AI_AGENT`, `is_ai = true`) proposes an `aios.fs.write` action to update a shared family document at `/aios/groups/family/shared/notes/2026-05-11.md`. The bundle's `family.shared.write_by_ai` rule classifies this as `REQUIRE_APPROVAL`.

**Trace differences from §4.**

- **Step 8.** The intent comes from the agent's planning loop (S13.1), not the operator. `subject = "family:family-assistant"`, `is_ai = true`.
- **Step 10.** Envelope's `request.subject = "family:family-assistant"`, `idempotency_key` is agent-generated.
- **Step 12.** `EvaluatePolicy` returns `REQUIRE_APPROVAL` per S2.3 §3 step 8 (scoped allow with approval requirement) and S2.3 §17 (AI self-approval prevention applies — the approver must be human). `ApprovalRequirement{ approver_classes = ["human"], strength = STRONG, ttl_seconds = 300 }`.
- **New step 12.5 — Approval flow.** Per S5.3:
  - `ApprovalRequest` constructed with `target_action_canonical_hash = H` per S5.3 §4 + §13.2; state = `DRAFT`.
  - Channel selection: `KDE_NATIVE_PROMPT` per S5.3 §6 (operator alice is at the local KDE console).
  - Prompt rendered as `APPROVAL_PROMPT` `NodeKind` in CHROME zone (S7.4 §4 + S7.1 INV-023 — the prompt is `AIOS_SURFACE` authored by `aios_chrome`, not by the agent).
  - State transitions: `DRAFT → AWAITING_OPERATOR → GRANTED`. Alice approves with hardware key (`STRONG` strength).
  - `ApprovalBinding` issued per S5.3 §5: `bound_action_canonical_hash = H`, `expires_at = now + 300s`, `scope = EXACT_ACTION`, single-use.
  - Lifecycle: `APPROVAL_PENDING → APPROVED` (S10.1 §3.1).
  - Evidence: `APPROVAL_REQUESTED`, `APPROVAL_DELIVERED`, `APPROVAL_GRANTED` (all per S5.3 §10 / S3.1 §25.2; LONG retention floor).
- **Step 13.** Eight-step pre-dispatch:
  - Step 1: re-validate canonical hash against `bound_action_canonical_hash`. Match ✓.
  - Step 7: mark `ApprovalBinding` as `CONSUMED` per S5.3 §13.1. Evidence: `APPROVAL_CONSUMED`.
  - Step 8: dispatch under `ISOLATED_SANDBOX` (subject `is_ai = true` forces it per S10.1 §3.2).
- **Step 14.** AIOS-FS write under `ISOLATED_SANDBOX` (the adapter runs sandboxed); transaction commits identically.
- **Step 15.** Verification identical.
- **Step 16.** Evidence chain expanded with the approval triplet (`APPROVAL_REQUESTED`, `APPROVAL_DELIVERED`, `APPROVAL_GRANTED`, `APPROVAL_CONSUMED`).
- **Step 17.** Renderer applies `COLOR_ACTION_AI` token (INV-021) to the action row — the operator sees at a glance that this row is AI-authored. The approval is visible in the chain as a HUMAN-authored sub-row (alice's grant).

**INV citations exercised that are NOT in §4.**

- INV-002 (AI proposes never executes) — the agent submitted, the operator approved, the runtime executed.
- INV-009 (approvals bind to one request and expire) — the binding is `EXACT_ACTION`, single-use, 5-minute TTL.
- INV-010 (AI cannot self-approve) — the `approver_classes = ["human"]` constraint is the constitutional enforcement.
- INV-021 (AI vs human visual distinct) — the renderer treats AI-row differently from human-row.

**Acceptance.** The trace forms a complete DAG with 14+ receipts (vs 11 in §4); INV-002, INV-009, INV-010, INV-021 all exercised; operator visually distinguishes AI vs human.

**Honesty.** Fully specified. The approval flow walks S5.3 + S10.1 + S7.4 + S3.1 cleanly.

### §7.2 Scenario B — Operator initiates recovery

**Setup.** Operator boots into recovery mode (GRUB entry 2: `AIOS Recovery`) per S9.1 §4.1. Reason: `OPERATOR_INITIATED`. Goal: rotate the policy bundle in `/aios/system/policy/` (a `RecoveryMutableScope.POLICY_BUNDLE` mutation).

**Trace differences from §4.**

- **Step 1 (boot).** GRUB selects entry 2; kernel command line `aios.mode=RECOVERY; aios.recovery_reason=OPERATOR_INITIATED`.
- **Step 2 (`STAGE_L0_GOVERNANCE_READY`).** Identical.
- **Step 3 (`STAGE_L4_DEGRADED_READY`).** Identity service in **degraded mode** (only `_system` scope subjects per S9.1 §3.5). Vault broker in degraded mode (key-management ops only; no normal-mode capability issuance per S9.1 §9.3).
- **Step 4 (`/aios` mount).** **`/aios` is NOT mounted** as a full projection per S9.1 §5.1; only the `system/...` recovery projection is mounted with rw access for `RecoveryMutableScope` paths.
- **Step 5 (auth).** Operator authenticates with hardware key (S9.1 §6.2). Subject: `_system:remote:operator-247` (or `_system:local:operator-247` for local console). `SessionClass = RECOVERY` per S5.1 §8.1; `is_recovery_mode = true`; `expires_at = authenticated_at + 8h` per S9.1 §8 (the constitutional 8-hour cap).
- **Step 6 (services).** L3 SGR is up but **L5 services are masked** per S9.1 §9; no agent runtime, no planner, no model server. The recovery shell is an `AIOS_SURFACE`-only KWin session per S7.1 §6 + S7.4 §7. The aesthetic is the recovery-mode theme (INV-022) — visually unmistakable from normal mode.
- **Step 7 (renderer).** Recovery shell renders the operator console; only `AIOS_SURFACE` permitted (S7.1 I6); `APP_SURFACE`/`STREAM_SURFACE` rejected with `RecoveryModeKindForbidden`.
- **Steps 8–10.** Goal is typed directly (no L5 translator in recovery — INV-001). Operator types a `policy.bundle.update` action via the recovery `aios-cli-recovery` tool per S9.1 §9.3.
- **Step 11.** `ValidateAction` runs identically.
- **Step 12.** `EvaluatePolicy` evaluates the action with `subject.recovery_mode = true`. The hard-deny `RecoveryRequiredForSystemMutation` (S2.3 §26.2.2 / S4.1 §12.4) is **NOT triggered** because the subject is recovery-mode; the kernel applies the per-scope rule for `RecoveryMutableScope.POLICY_BUNDLE` and returns `REQUIRE_APPROVAL` (the bundle requires a human approver even in recovery — INV-009 applies).
- **Step 12.5 — Approval flow.** A human approver (`_system:local:operator-247` or another recovery operator) approves through the recovery shell (`KDE_NATIVE_PROMPT` in recovery aesthetic).
- **Step 13.** Eight-step pre-dispatch with `subject.recovery_mode = true`. Dispatch via `SUBPROCESS_FORK` (operator is human, even in recovery).
- **Step 14.** The bundle file under `/aios/system/policy/` is updated atomically per S1.3 §8.2 CAS. INV-012 (recovery required for system mutation) is exercised — the constitutional gate is the recovery-mode flag, plus the human approver, plus the FOREVER `RECOVERY_EVENT` evidence.
- **Step 15.** Verification: the new bundle's signature is verified per S2.3 §12.3.
- **Step 16.** Evidence chain includes a FOREVER `RECOVERY_OPERATION_PERFORMED` (S9.1 §12) and a FOREVER `RECOVERY_EVENT` per S3.1 §13. INV-005 holds even in recovery — the log is read-only-verification in recovery; the kernel appends on behalf of the operator's RPC, never directly.
- **Step 17.** Recovery shell shows the operation result. After completing operations, operator issues `recovery-reboot` per S9.1 §10.3.
- **Step 18 (reboot).** The host reboots into NORMAL mode; the new bundle is picked up at NORMAL boot's policy-load stage.

**INV citations exercised that are NOT in §4.**

- INV-001 (recovery independent of L5) — no L5 service runs in recovery; the operator types a typed action directly.
- INV-012 (recovery required for system mutation) — the policy bundle mutation is gated on `is_recovery_mode = true` + human approver + FOREVER evidence.
- INV-022 (recovery aesthetic distinct) — the renderer uses the recovery-only theme; visually unmistakable.
- S5.1 §8 `SessionClass.RECOVERY` — separate principal class.

**Acceptance.** The trace exits via `REBOOT_TO_NORMAL` per S9.1 §3.4 (the only normal exit path). The new bundle takes effect on the next NORMAL boot's policy-load stage. INV-001, INV-012, INV-022 all exercised.

**Honesty.** Fully specified — S9.1 §1..§16 covers the recovery boundary in concrete terms.

### §7.3 Scenario C — Adversarial: AI agent tries to install a package directly

**Setup.** AI agent `family:family-assistant` proposes a `package.install` action targeting `/aios/system/apps/<app_id>/`. The agent is attempting to install a system-scope app directly.

**Trace differences from §4.**

- **Step 8.** Agent's planning loop emits the proposal.
- **Step 9.** S1.1 translator compiles to `package.install` with `target.path = /aios/system/apps/<app_id>/` and `target.scope = SYSTEM`.
- **Step 10.** Envelope built. `subject = family:family-assistant`, `is_ai = true`.
- **Step 11.** `ValidateAction` succeeds (the action and target schema are valid; the violation is policy-level, not validation-level).
- **Step 12.** `EvaluatePolicy` runs the decision pipeline:
  - Step 5 of S2.3 §3 (evaluate hard denies) returns the hard-deny `AISystemAdminBlocked` per S2.3 §26.2.3 / S4.1 §12.4: **subject.is_ai = true AND target.scope = SYSTEM AND target.system_reserved ∈ {SYS_APPS, SYS_AGENTS} ⇒ DENY**.
  - The decision is `DENY` with `reason_code = "AISystemAdminBlocked"`.
  - INV-013 (AI cannot perform system admin) is the constitutional invariant.
  - INV-002 (AI proposes never executes) is the broader principle; INV-013 is the specific mechanical enforcement at this seam.
- **Step 12.5.** Lifecycle transitions: `CREATED → POLICY_PENDING → POLICY_DENIED` (terminal).
- **Step 12.6 — Override path attempt.** Operator could attempt an emergency override per S5.4. **However**, the hard-deny rule `AISystemAdminBlocked` is in `NonOverridableClass` per the constitutional design (it's tied to INV-013 which "cannot be loosened by any policy bundle or capability binding"). The override path returns `TARGET_NOT_OVERRIDABLE` per S5.4 §3.5. Lifecycle would transition `POLICY_DENIED → OVERRIDE_DENIED` if the operator tried.
- **Step 16.** Evidence chain:
  - `ACTION_RECEIVED` (STANDARD_24M).
  - `ACTION_VALIDATED` (STANDARD_24M).
  - `POLICY_DECISION` with decision = DENY at FOREVER retention (per S3.1 §13 row 1).
  - `ACTION_POLICY_DECISION` (STANDARD_24M).
  - The chain ends at `POLICY_DENIED`; no execution.
- **Step 17.** The renderer (operator's agent dashboard, an `AIOS_SURFACE`) shows the proposal with status `POLICY_DENIED`, reason `AISystemAdminBlocked`, with `COLOR_ACTION_AI` (INV-021) and the evidence chain link. The operator sees that the agent **proposed** an action and the system **fail-closed denied** it — exactly the constitutional posture.

**Defense in depth.** Even if the operator authored an override request, the hard-deny class is non-overridable per S5.4 §10 `NonOverridableClass`. Even if the policy bundle were tampered with to relax `AISystemAdminBlocked`, the bundle compiler at S2.3 §12.3 would reject the bundle with `InvariantLooseningAttempted` per L0.4 §5.3. Even if the bundle compiler were bypassed, INV-013's enforcer is hard-coded in S2.3 §17 and is not a bundle rule.

**INV citations exercised that are NOT in §4.**

- INV-002 (AI proposes never executes) — agent proposed, runtime denied.
- INV-013 (AI cannot perform system admin) — the constitutional gate.
- INV-021 (AI vs human visual distinct) — operator sees AI-authored row distinctly.
- L0.4 §5.3 (layered loosening rejected) — defense in depth.

**Acceptance.** The trace ends at `POLICY_DENIED` with a FOREVER evidence record. The agent cannot install a package directly; the operator can install one via an approved typed action through the normal flow. INV-002 + INV-013 + L0.4 layered defense all exercised.

**Honesty.** Fully specified — S2.3 §6, §17, §26 + S4.1 §12.4 + L0.4 §3 + S5.4 §3.5, §10 + L0.4 §5.3 cover the defense-in-depth chain.

## §8 Open deferrals (out of scope for MVP golden path)

The MVP self-validation explicitly **excludes** the following — they are real spec items but are not on the golden path:

- **Multi-host federation.** A single AIOS host runs the canonical trace. Multi-host coordinated actions, federated identity, federated policy bundles are out of scope per S5.1 §19, S2.3 §22, S10.1 §17.
- **Voice and mobile renderers.** L7.6 (Voice) and L7.7 (Mobile) are `DEFERRED` per the master index. The MVP renderer is KDE; secondary is Web (S7.5 — `CONTRACT`).
- **Marketplace UI.** L10.2 marketplace is `SHELL`. The repository model (S11.1) is `CONTRACT` and supports the trust chain for adapter manifests, but the marketplace UI itself is out of scope.
- **External AI provider full integration test.** S13.1 mentions external providers; L8.1 §J specifies the network policy for outbound calls; full E4 integration test is post-MVP. The MVP scenario is fully local (no external AI).
- **Wine/Proton/Waydroid runtimes.** S12.1 specifies `RUNTIME_LINUX_NATIVE`, `RUNTIME_WINE`, `RUNTIME_WAYDROID`, `RUNTIME_VM` as closed enum values, but the MVP scenario uses `RUNTIME_LINUX_NATIVE` only.
- **GPU compute workloads.** L8.2 (GPU resource model) is `CONTRACT` and INV-024 governs GPGPU access, but the MVP scenario uses only basic 2D rendering — no `GPU_COMPUTE_HEAVY` path.
- **Vault `RevealSecret` flow.** S5.2 specifies the recovery-mode reveal path, but the MVP scenario does not touch raw secret material (INV-018 holds trivially because no vault op is invoked).
- **Conflict resolution.** S1.3 conflict resolution (`03_conflict_resolution.md`) handles concurrent writes; the MVP scenario is single-write, no conflict.
- **Quarantine / GC under load.** S1.3 §12 (quarantine) and §7.3 (GC) are exercised in §11.3 worked failure trace but not in the canonical happy path.
- **Dedicated kernel pipeline.** S9.3 is `CONTRACT` but the MVP boots the generic fallback kernel (entry 1 of S9.1 §4.1).
- **Emergency override (non-`NonOverridableClass`).** S5.4 is `CONTRACT`. Scenario C in §7.3 touches the override path but only to demonstrate `NonOverridableClass` rejection.

These deferrals are **deliberate**: the MVP golden path is the smallest test that exercises every constitutional layer once. Adding any of the above would expand the test without adding constitutional coverage.

## §9 The MVP acceptance test as a verifiable artifact

When AIOS is implemented (E2 → E3 → E4 → E5), the MVP acceptance test runs the canonical §4 trace. The test passes iff:

1. **All 18 step outputs match.** Each step's declared output (per §4) is observed in the running system. Subjects, action ids, object ids, version ids, pointer ids, transaction ids are all materialized as ULID-prefixed canonical identifiers per their respective sub-specs.
2. **All evidence records are emitted.** The eleven canonical receipt types from §4.16 + the surface-lifecycle records from §4.7 + the auth records from §4.5 are all appended to L9.1 with correct retention class, correct subject linkage, and correct chain hashes.
3. **Total evidence chain is a single signed DAG.** `VerifyChain` (S3.1 §17) returns `chain_valid = true` for every segment containing a receipt for `correlation_id = corr_<ULID>`. Every receipt's `previous_receipt_hash` resolves to a real prior receipt within its segment; cross-segment via `CHAIN_CHECKPOINT`.
4. **Renderer artifact (step 17) is visible to the operator.** A QA observer at the KDE console sees the evidence viewer's CONTENT zone show the journal entry row, with `COLOR_ACTION_HUMAN` token, with the AIOS chrome zone above unobstructed.
5. **Total trace time ≤ X seconds (operator latency budget).** The user-task contract leaves X open; per S0.1 §10 + S2.3 §18 + S2.4 §15 + S10.1 §14 budgets, a reasonable end-to-end target is **≤ 2 seconds** from envelope submission (step 10) to surface update (step 17), excluding the operator's typing time and approval reaction time. For non-approval actions like §4 the budget is tighter: **≤ 500 ms** from `SubmitAction` to `STATUS_TRANSITION SUCCEEDED` evidence on a reference workstation. This budget is queued for the MVP acceptance harness.

This makes "MVP achieved" mechanically checkable, not aspirational. The acceptance test is itself a closed enum of pass/fail predicates; running it against an implementation produces a deterministic answer.

## §10 Cross-spec dependency map (audit map for the spec)

Comprehensive dependency table tracing every step's spec dependencies. **If any cell of this table cannot be filled, that's a finding for the audit phase.** All cells below are filled; the spec is consistent with the trace.

| Step | Layer    | Sub-spec          | Section               | Status (per master index)        | Closed enums consumed                                                                                                                                                                                               | INV cited               | Evidence record types                                                                                     |
| ---- | -------- | ----------------- | --------------------- | -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- | --------------------------------------------------------------------------------------------------------- |
| 1    | L1       | S9.1              | §4.1, §4.2, §3.5      | `REAL`                           | `RecoveryStage`, `RecoveryMode`                                                                                                                                                                                     | 001, 007                | (none at this stage)                                                                                      |
| 2    | L0       | L0.4              | §3, §4                | `CONTRACT`                       | `InvariantId`, `InvariantBundle`                                                                                                                                                                                    | I1, I3, I5              | `INVARIANT_BUNDLE_LOADED` FOREVER                                                                         |
| 3    | L4       | S5.1              | §3, §8                | `CONTRACT`                       | `SubjectKind`, `SessionClass`, identity bundle                                                                                                                                                                      | S5.1 I2, I8             | `IDENTITY_BUNDLE_LOADED` FOREVER (Wave 6)                                                                 |
| 4    | L2       | S1.3, S2.2        | §13, §D9              | `CONTRACT`                       | namespace catalog version (S4.1 §13)                                                                                                                                                                                | 004, 007                | `STORAGE_INITIALIZED` (Wave-N)                                                                            |
| 5    | L4       | S5.1              | §3, §8                | `CONTRACT`                       | `SubjectKind`, `SessionClass`                                                                                                                                                                                       | S5.1 I1, I6             | `IDENTITY_SUBJECT_AUTHENTICATED` STANDARD_24M                                                             |
| 6    | L3+L9+L7 | S10.1, S3.1, S7.4 | §3, §13, §10          | `CONTRACT`+`CONTRACT`+`CONTRACT` | `ActionLifecycleState`, `RecordType`                                                                                                                                                                                | 002, 005, 019, 020, 023 | `RUNTIME_INITIALIZED`, `EVIDENCE_LOG_OPENED`, `KDE_RENDERER_STARTED` STANDARD_24M                         |
| 7    | L7       | S7.1, S7.4        | §3, §6, §11 Fix.1     | `CONTRACT`                       | `SurfaceKind`, `CompositionZone`, `SurfaceLifecycle`                                                                                                                                                                | 019, 020, 021, 023      | `SURFACE_CREATED` STANDARD_24M                                                                            |
| 8    | L5       | S13.1, S1.1       | §6, §D2               | `CONTRACT`                       | (S13.1 intent record)                                                                                                                                                                                               | 002, 007                | `TRANSLATION_CREATED` STANDARD_24M                                                                        |
| 9    | L5       | S1.1, S10.1       | §D1..D9, §10.1        | `CONTRACT`                       | `AdapterIOMode`, `RollbackStrategy`, target_schema                                                                                                                                                                  | 002, 007                | (covered by `TRANSLATION_CREATED`)                                                                        |
| 10   | XX       | S0.1              | §2..§4, §8            | `CONTRACT`                       | `Phase`, `ConditionStatus`, `Environment`, `DryRunMode`, `Risk`                                                                                                                                                     | 002, 014                | (none — emission starts at step 11)                                                                       |
| 11   | L3       | S10.1             | §5, §6.1, §10         | `CONTRACT`                       | `ActionLifecycleState`, `ActionDispatchKind`, `AdapterStability`, `QueueClass`, `ExecutionFailureReason`, `RuntimeErrorCode`, target_schema (manifest), namespace `ResolutionErrorCode`, verification primitive set | 007, 014                | `ACTION_RECEIVED`, `ACTION_VALIDATED` STANDARD_24M                                                        |
| 12   | L4       | S2.3              | §3..§13, §17, §26     | `CONTRACT`                       | `Decision`, `ApprovalRequirement`, `ConditionsField`, `Constraints`, hard-deny ids, namespace conditions                                                                                                            | 002, 008, 010, 011, 013 | `POLICY_DECISION` STANDARD_24M (FOREVER for DENY/REQUIRE_APPROVAL), `ACTION_POLICY_DECISION` STANDARD_24M |
| 13   | L3       | S10.1, S3.2       | §6, §3.2; §3, §5      | `CONTRACT`                       | `ActionDispatchKind`, sandbox composition source enum                                                                                                                                                               | 002, 014, 017           | `ACTION_DISPATCHED`, `SANDBOX_PROFILE_APPLIED` STANDARD_24M                                               |
| 14   | L2       | S1.3, S4.1        | §5..§9, §8.2; §8.2    | `CONTRACT`                       | `ObjectKind`, `VersionState`, `PointerKind`, `TransactionState`, `PrivacyClass`, `NamespacePath`, `ScopeKind`                                                                                                       | 004, 005, 011           | `EXECUTION_STARTED`, `AIOSFS_TRANSACTION_COMMITTED` STANDARD_24M                                          |
| 15   | L9       | S2.4, S10.1       | §4, §6; §7.1          | `CONTRACT`                       | `VerificationStatus`, `PropertyType`, primitive vocabulary                                                                                                                                                          | 014                     | `VERIFICATION_RESULT` 180d, `EXECUTION_SUCCEEDED` STANDARD_24M                                            |
| 16   | L9       | S3.1              | §3, §5, §11, §13, §17 | `CONTRACT`                       | `RecordType` (205 entries), retention class (`STANDARD_24M`/`EXTENDED_60M`/`FOREVER`)                                                                                                                               | 005, 014, 015           | (DAG aggregate of all preceding records)                                                                  |
| 17   | L7+L9    | S3.1, S7.1, S7.4  | §9; §6.2; §4, §11     | `CONTRACT`                       | `Subscribe` RPC, `CompositionZone`, `SurfaceKind`, `NodeKind`, color tokens                                                                                                                                         | 019, 020, 021, 023      | (consumes evidence; emits `SURFACE_LIFECYCLE` events as needed)                                           |
| 18   | L7       | S7.4              | §11                   | `CONTRACT`                       | (operator UX)                                                                                                                                                                                                       | 014                     | `SURFACE_DESTROYED` STANDARD_24M (on dismiss)                                                             |

**Audit-map check.** Every cell is filled. Every `Status` is `CONTRACT` or `REAL`. Every cited section exists in its sub-spec. Every closed enum exists in its sub-spec. Every INV cited exists in L0.4. Every evidence record type exists in S3.1's narrative (post-Wave 7: 205 entries) or in a sub-spec's queued evidence vocabulary. **The trace is closed.**

## §11 Worked failure traces

Three concrete adversarial paths showing the spec handles failure cleanly.

### §11.1 Failure trace — Step 11 ValidateAction: malformed envelope

**Scenario.** Operator (or a client tool) submits an envelope with a malformed `target.content` field — the field is missing required `"text/markdown"` mime-type metadata that the AIOS-FS adapter's `target_schema` requires.

**Walk.**

- **Step 10.** Envelope built. `request_hash` computed.
- **Step 11.** `ValidateAction` runs:
  - Schema validation (envelope conforms to `aios.action.v1alpha1`) ✓.
  - **Target validation fails.** The adapter's `target_schema` (per S10.1 §10.1) requires `target.content_mime` for `aios.fs.write`; the field is absent.
  - Validator emits `ENVELOPE_VALIDATION_FAILED` per S10.1 §3.6.
  - Lifecycle: `(init) → CREATED → FAILED` per S10.1 §3.1. `Error.code = "EnvelopeValidationFailed"` per S0.1 §7.3 canonical taxonomy.

- **Step 16.** Evidence chain (truncated):
  - `ACTION_RECEIVED` STANDARD_24M.
  - `ACTION_VALIDATED` with `result = FAILED` STANDARD_24M.
  - `EXECUTION_FAILED` with `ExecutionFailureReason = ENVELOPE_VALIDATION_FAILED` (per S10.1 §13; STANDARD_24M floor, may be EXTENDED_60M for forensics).

- **Step 17.** The renderer's evidence viewer surface (subscribed to the operator's stream) shows the failed action with status `FAILED`, reason `EnvelopeValidationFailed`, and a click-through to the validator's specific error message: `"target.content_mime is required for aios.fs.write"`.

**INV exercised.** INV-014 (no proof, no completion) — the action does not claim completion; it claims `FAILED`. INV-005 — the failure record is appended, never overwritten.

**Acceptance.** Failure path is clean: the action is `FAILED`, evidence is appended, the operator sees the reason, no partial state was created in AIOS-FS (because dispatch never happened).

### §11.2 Failure trace — Step 12 EvaluatePolicy: AI on INTERACTIVE queue silently downgrades

**Scenario.** AI agent `family:family-assistant` submits an action with `request.environment = LOCAL` and (via a buggy or adversarial client tooling) attempts to declare the queue class as `INTERACTIVE`. Per S10.1 §3.5 the queue class is decided by the runtime, not by the client, but a misbehaving caller may set client-side hints attempting `INTERACTIVE` priority.

**Walk.**

- **Step 11.** `ValidateAction` succeeds (subject, target, schema all valid).
- **Step 12.** `EvaluatePolicy` runs the decision pipeline. For this scenario, assume the decision is `ALLOW` (the agent is permitted to write to its own scope; no approval required — rare but valid for low-risk agent self-management).
  - Lifecycle: `CREATED → POLICY_PENDING → APPROVED`.
- **Step 12.5 — Queue selection.** The runtime selects the queue class per S10.1 §3.5:
  - Subject is human? `False` (`is_ai = true`).
  - The S10.1 §3.5 selection rule routes AI subjects to `AGENT_PROPOSAL`, not `INTERACTIVE`.
  - The runtime applies the **silent downgrade** per S10.1 §11.4: "AI subjects attempting to submit on INTERACTIVE are silently downgraded to AGENT_PROPOSAL and an `AI_INTERACTIVE_QUEUE_DOWNGRADE` evidence record is emitted."
  - Lifecycle: `APPROVED → QUEUED` (in `AGENT_PROPOSAL` queue).

- **Step 16 (partial).** Evidence:
  - `ACTION_POLICY_DECISION` STANDARD_24M.
  - `AI_INTERACTIVE_QUEUE_DOWNGRADE` per S10.1 §13 STANDARD_24M (forensically visible — the audit reveals every downgrade attempt).

- **Steps 13–17.** The action proceeds normally through dispatch, execution, verification, evidence. The user/operator sees no failure — the downgrade is silent at the action level. But the audit trail shows the attempt.

**INV exercised.** INV-002 (AI proposes never executes) — the agent's action goes through the runtime at AI-priority, not human-priority. INV-005 — the downgrade evidence is FOREVER-visible.

**Acceptance.** The downgrade is silent at the action level (no failure) but loud at the audit level (every downgrade is forensically visible). An operator running an audit query for `AI_INTERACTIVE_QUEUE_DOWNGRADE` events finds them.

### §11.3 Failure trace — Step 14 AIOS-FS write: orphan chunk after partial write

**Scenario.** During step 14, the AIOS-FS adapter writes a chunk for the journal entry but the operator's session terminates (network disconnect, KDE crash) before `CommitTransaction()` is called. The transaction stays in `PENDING_TX`; the chunk is on disk but referenced only by the staged transaction, not by any active version.

**Walk.**

- **Step 14 (partial).** Adapter wrote chunk `chk_<hex>`; transaction `txn_<ULID>` in `PENDING_TX`; no `CommitTransaction()`.
- **Step 14.5 — Transaction TTL.** Per S1.3 §9.2: a transaction in `PENDING_TX` for longer than its staging TTL (default 1 hour) is auto-aborted by the runtime. Lifecycle: `PENDING_TX → ABORTED`. Sibling versions persist as `STAGED`; pointers are unchanged (the CAS never happened, so no pointer was promoted).
- **Step 14.6 — Orphan chunk eligibility.** Per S1.3 §7.3: chunks written by an aborted transaction become orphan-eligible after the orphan staging TTL of 24 hours. Until then, the chunk remains on disk in case the transaction is retried.
- **Step 14.7 — GC pass.** After 24h, a scheduled or operator-initiated GC pass (S1.3 §7.3) reaps orphan chunks. The chunk's `ref_count = 0`; no staged transaction holds it (the transaction was aborted >23h ago and any retry would have created a new transaction). Chunk is reaped.
- **Step 16.** Evidence:
  - `EXECUTION_STARTED` STANDARD_24M (from step 14 partial).
  - `TRANSACTION_ABORTED` (queued in S3.1 — exact name in S1.3's Wave-N evidence vocabulary; STANDARD_24M).
  - `EXECUTION_FAILED` with `ExecutionFailureReason = ADAPTER_TIMEOUT` or `ADAPTER_PANIC` STANDARD_24M.
  - 24h later: `GC_PASS` per S3.1 §4 entry 15 STANDARD*24M, with `reaped_chunks = [chk*<hex>]` per S1.3 §7.3 — "GC is not silent. Each GC pass writes an evidence record."

- **Step 17.** Renderer shows the action as `FAILED`. Operator can retry; retry creates a new envelope with a new `idempotency_key` (the prior was tied to the failed transaction).

**INV exercised.** INV-005 — both the failure and the GC are evidence-logged; nothing silent. INV-014 — no proof, no completion; the action is `FAILED`, not `SUCCEEDED`. S1.3 §D4 (chunking + GC contract) — orphans are reaped on a logged schedule, not silently.

**Acceptance.** The system handles partial-write failure cleanly: transaction aborts after TTL; orphan chunk is reaped after a separate TTL with evidence; no partial state contaminates the live store; operator can retry.

## §12 Status & evidence grade

**Status:** `REAL`
**Evidence:** `E1`

E1 evidence: this file exists; it is the structural contract for the MVP self-validation; it walks the §22 trace step-by-step through every layer and cites a CONTRACT-grade or REAL sub-spec section for each step; it surfaces three minor gaps (§Step.3, §Step.4 from the S9.2 deferral; §Acceptance.4 from the L9.3 deferral); the canonical trace forms a complete signed DAG; the acceptance criteria are 6 of 7 ticked with the single gap being a non-blocking consolidation gap.

The next evidence step (E2) requires the §10 dependency-map table to be encoded as a machine-readable artifact (YAML or proto) consumable by an audit harness that enumerates every cell and verifies citation existence. The next step (E3) requires unit-level verification of the §5 acceptance checklist against an implementation skeleton. The next step (E4) is the MVP acceptance test running the §4 trace end-to-end against a working AIOS host with all evidence reconstructible from L9.1. The full E5 (live operational) status is reached only after a real operator runs the canonical scenario and the trace produces all eleven canonical receipts plus surface lifecycle records, with `VerifyChain` returning `chain_valid = true`, on a deployed AIOS host.

## See also

- [Rev.1 §22 — MVP Scope](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
- [L0.4 — Constitutional Invariants (INV-001..INV-024)](../L0_Governance_Evidence_Safety/04_invariants.md)
- [S9.1 — Recovery Boundary](../L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md)
- [S1.3 — AIOS-FS Object Model](../L2_AIOS_FS/01_object_model.md)
- [S2.1 — AIOS-FS Query/View Language](../L2_AIOS_FS/02_query_view_language.md)
- [S4.1 — Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S5.1 — Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S5.3 — Approval Mechanics](../L4_Policy_Identity_Vault/04_approval_mechanics.md)
- [S0.1 — Action Envelope + Lifecycle](01_action_envelope_lifecycle.md)
- [S10.1 — Capability Runtime gRPC](../L3_AIOS_SGR_Service_Graph_Runtime/03_capability_runtime_grpc.md)
- [S2.4 — Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S7.1 — Surface + Composition Model](../L7_Interaction_Renderers/01_surface_composition.md)
- [S7.4 — KDE Plasma Renderer](../L7_Interaction_Renderers/04_kde_renderer.md)
- [S13.1 — Cognitive Core Model](../L5_Cognitive_Core/01_cognitive_core_model.md)
- [S1.1 — Capability Translator](../L5_Cognitive_Core/02_capability_translator.md)
- [S3.2 — Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [S11.1 — Repository Model + Trust Roots](../L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [XX_Cross_Cutting Overview](00_overview.md)
