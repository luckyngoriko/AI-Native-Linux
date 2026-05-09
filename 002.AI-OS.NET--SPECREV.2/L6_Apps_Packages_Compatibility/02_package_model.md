# Package Object Model — On-Disk Layout, Versioning, Update, Rollback (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists, structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Phase tag      | S12.2                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Layer          | L6 Apps, Packages, Compatibility                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Schema package | `aios.package.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Consumes       | L0 INV-004 (recovery boundary), INV-011 (cross-group access forbidden), INV-014 (no proof, no completion), INV-017 (sandbox floor constitutional); S0.1 Action Envelope + Lifecycle (`ActionEnvelope`, `ActionId`, JCS canonicalization, `BLAKE3` digest discipline); S1.3 AIOS-FS object model (versioned content-addressed objects, optimistic concurrency, `ConflictDetected`, pointer/chunk separation); S2.3 Policy Kernel (default-deny, `CrossGroupAccessForbidden`, `RecoveryRequired`); S2.4 Verification Grammar (`PropertyType`, `aiosfs_path_in_namespace`, `SANDBOX_PROFILE_MOST_RESTRICTIVE`); S3.1 Evidence Log (`RecordType` vocabulary, retention classes `STANDARD_24M` / `EXTENDED_60M` / `FOREVER`); S3.2 Sandbox Composition (`SandboxProfile`, six-source merge, runtime safety floor); S4.1 Namespace Layout (`/aios/system/apps/<app_id>/`, `/aios/groups/<group_id>/apps/<app_id>/`, `installable_scope` constraint, queued user-scope addition); S11.1 Repository Model + Trust Roots (`PackageManifest` skeleton, seventeen-step install pipeline, `PackageInstallState`, `PackageVerificationResult`, `PackageKind`, `InstallScope`, `PublisherTrustLevel`, `RepositoryKind`, `UpdateChannel`); S12.1 App Runtime Model (`EcosystemRuntime`, `RUNTIME_LINUX_NATIVE`, `RUNTIME_WINDOWS_PROTON`) |
| Produces       | the closed `PackageObjectKind` enum (eight values); the closed `PackageContentKind` enum (ten values); the closed `PackageObjectState` enum (eight values); the closed `RollbackKind` enum (four values); the closed on-disk layout per package object (twelve required artifacts, six optional artifacts); the binding from S11.1 `PackageInstallState` to S12.2 `PackageObjectState`; the staged-update flow with thirty-day supersede retention; the per-package private state directory contract under sandbox confinement; the rollback policy that forbids restoration to known-vulnerable versions; ten evidence record types queued for S3.1; one cross-spec touch-up to S4.1 introducing `apps/` under user scope; one cross-spec touch-up to S2.4 declaring property `PACKAGE_OBJECT_LAYOUT_INTACT`; bounded-cardinality telemetry contract                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |

## §1 Purpose

S11.1 defines _how a package arrives on the host_ — the trust chain, the manifest contract, the seventeen-step install pipeline, the install-time `PackageInstallState` FSM, the verification outcomes, the publisher-key rotation, and the deplatform discipline. S12.1 defines _which runtime executes the binaries_ — the closed `EcosystemRuntime` enum, the AI-assisted four-phase setup, and the per-runtime sandbox floors.

Neither sub-spec answers the next question: **once a package has been admitted, where does it physically live on AIOS-FS, what files are required to be present, what is the persistent state machine for that on-disk object, how do staged updates and rollback work, and how is the package's private writable state isolated?** That is the loop S12.2 closes.

This contract is the **on-disk truth** of an installed package. Every other layer that reads or mutates a package object — the L7 marketplace surface (`is this app installed? at what version?`), the L4 policy kernel (`does the action target a quarantined package?`), the L9 evidence log (`which package emitted this record?`), the L1 recovery path (`what packages does the recovery operator need to enumerate before the AI subsystems are available?`) — consumes the closed enums, the layout table, and the state machine declared here.

The contract is constrained by five constitutional risks:

1. **Package content tampering on disk** — a package object that passed install-time signature verification is later corrupted on disk (bit rot, attacker with filesystem access, compromised host process). Addressed by per-load content-hash verification (§9) with FOREVER evidence on mismatch.
2. **Private state poisoning** — an actor outside the package's sandbox writes to the package's private state directory, corrupting application state or planting hostile data. Addressed by sandbox-enforced restriction of `PRIVATE_STATE_DIR` to the owning package's subject identity (§7) with FOREVER evidence on cross-package writes.
3. **Rollback to vulnerable version** — an operator (or an attacker manipulating the operator) requests rollback to a prior version that has been flagged as containing a known vulnerability. Addressed by the `RollbackBlocklist` consulted at rollback time (§10.4) with FOREVER `PACKAGE_VERSION_DOWNGRADE_BLOCKED` evidence; CVE flagging uses S11.1 publisher-reputation feed.
4. **Staged-update promotion abuse** — an attacker stages a hostile update under a legitimate package id and tries to promote it to `ACTIVE` without re-running the install pipeline. Addressed by the §10.2 promotion contract: every promotion is itself an action envelope routed through S2.3 policy plus S11.1's verification step; promotion never bypasses the install pipeline.
5. **Recovery-mode package skew** — `/aios/system/apps/` contains packages that cannot be enumerated until the AIOS-FS layer is up; recovery may need to reach a package object before the cognitive subsystem (L5) is reachable. Addressed by §3 layout constraints (closed file set, deterministic ordering, no L5 dependency for object enumeration).

Risk #1 is the most subtle: install-time signature verification is necessary but not sufficient. A package object on disk lives for the package's entire lifetime (potentially years), is read by many subjects (the renderer, the runtime, the policy kernel, the evidence log query layer), and any of those reads can be the trigger for a malicious modification slipping through unnoticed. The contract therefore requires **content-hash verification at every load**, not just at install time.

## §2 Scope

This spec **defines**:

1. The closed `PackageObjectKind` enum with eight values.
2. The closed `PackageContentKind` enum with ten values.
3. The closed `PackageObjectState` enum with eight values, distinct from S11.1 `PackageInstallState` (which is _install-time_; this is _post-install_).
4. The closed `RollbackKind` enum with four values.
5. The on-disk layout under `/aios/system/apps/<package_id>/`, `/aios/groups/<group_id>/apps/<package_id>/`, and (queued S4.1 touch-up) `/aios/groups/<group_id>/users/<user_id>/apps/<package_id>/` — the closed table of expected files per package object kind.
6. The mapping from S12.2 `PackageObjectState` to S11.1 `PackageInstallState` (every transition has exactly one corresponding install-state expectation).
7. The contract that every package object is itself a versioned AIOS-FS object per S1.3 (every file inside the layout is a content-addressed AIOS-FS object pointer; the package directory is itself an AIOS-FS object pointer; the directory's version is bumped on any child mutation).
8. The staged-update flow: `STAGED_UPDATE` package object created → S11.1 verification → promotion to `ACTIVE` with prior `ACTIVE` becoming `SUPERSEDED`.
9. The supersede retention: `SUPERSEDED` package objects are retained for thirty calendar days from the moment of supersede, then automatically transitioned to `RETIRED` (post-30d). Within the thirty-day window, single-step rollback is available.
10. The rollback discipline: closed `RollbackKind` enum (`NEVER`, `SINGLE_STEP`, `MULTI_VERSION`, `RECOVERY_ONLY`), the rollback action envelope, the `RollbackBlocklist` consulted before a rollback target is accepted.
11. The private state directory contract: `PRIVATE_STATE_DIR` is bound to the owning package's subject identity, restricted by the package's `SandboxProfile`, never readable cross-package, never writable cross-package.
12. The verification probe contract: a package object includes signed `VERIFICATION_PROBES` artifacts that the package itself shipped — used by S2.4 verification grammar to confirm install integrity post-install.
13. The evidence bundle contract: a package may ship an `EVIDENCE_BUNDLE` artifact that captures upstream attestations (build provenance, third-party security audits, SBOM); the bundle is content-addressed and reproducible; it is never the substitute for AIOS evidence (which only the host emits).
14. The quarantine flow: a `PackageObjectState = QUARANTINED` object cannot be loaded, executed, mutated, or rolled-back-into; the only legal exits are operator-driven `UNINSTALLING` (forward) or recovery-mode `RECOVERY_RESTORE` (back to a known-good prior version).
15. Adversarial robustness: package content tampering, private state poisoning, downgrade-to-vulnerable-version, staged-update promotion abuse, recovery-mode skew, concurrent install/uninstall race.
16. Bounded-cardinality telemetry contract.
17. Ten evidence record types queued for S3.1.
18. Three worked examples: Steam game install via `RUNTIME_WINDOWS_PROTON`; system service install referencing the queued S15.1 unit manifest; package update-with-rollback walkthrough.

This spec **does not** define:

- The wire format of `PackageManifest` — that is fixed in S11.1 §4. This spec consumes it; it does not redefine it.
- The seventeen-step install pipeline — that is fixed in S11.1 §5. This spec begins where the pipeline ends (state transitions to `INSTALLED`).
- The runtime selection algorithm — that is fixed in S12.1 §3. This spec records the runtime in the package object's manifest pointer; it does not select it.
- The sandbox composition algorithm — that is fixed in S3.2 §5. This spec stores the composed `SandboxProfile` artifact; it does not compose it.
- Marketplace UX, listing surface, install-button flow — that is L7's job (`SHELL`).
- Compatibility runtime orchestration (Wine prefix lifecycle, Waydroid container lifecycle, VM fallback) — that is `03_compatibility_runtime.md` (`SHELL`).
- The dedicated kernel A/B promotion pipeline (S9.3) — kernel candidate package objects record their layout here, but A/B promotion mechanics are deferred.
- The L5 capability catalog mutation flow — `CAPABILITY_CATALOG_DELTA` package objects record their layout here, but the catalog delta application is L5's job.
- Cross-host package state federation — every host owns its own `PackageObjectState`; no automatic propagation.
- Service unit manifest schema — referenced as the queued S15.1 contract; this spec only states that a service-class package's manifest pointer references the future S15.1 unit manifest object.

## §3 Vocabulary (closed enums)

All vocabularies in this section are closed. Adding a value is a versioned spec change. The package object loader, the rollback engine, and the quarantine engine MUST reject values outside the enum at parse time. None of these enums admits an `OPEN` or `OTHER` value.

### §3.1 `PackageObjectKind`

Closed enum, eight values. Every directory under `/aios/.../apps/<package_id>/` IS exactly one of these kinds. The kind is recorded in the package object's `meta.aios` file at install time and is immutable thereafter.

| Value                | Semantics                                                                                                                                                                                | Disk presence                                             | Pinned attributes                                                                                                             |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `INSTALLED_PACKAGE`  | A live, currently-active package on the host. Exactly one `INSTALLED_PACKAGE` may exist per `(install_scope, package_id)` pair. Loaded by runtimes; visible to L7 marketplace as active. | Permanent until uninstalled, superseded, or quarantined.  | `state ∈ {INSTALLED, ACTIVE}`; manifest pointer present; verification probes present; private state initialized.              |
| `STAGED_UPDATE`      | An update bundle that has passed S11.1 verification but has not yet been promoted to active. Coexists with the current `INSTALLED_PACKAGE` until promotion.                              | Until promotion or rejection.                             | `state = STAGED`; manifest pointer present; private state NOT yet bound.                                                      |
| `ROLLBACK_RESERVE`   | A snapshot of a previously-active package object retained for rollback. Created automatically when a `STAGED_UPDATE` is promoted (§10.2). Read-only.                                     | Thirty calendar days after creation, then auto-`RETIRED`. | `state = SUPERSEDED`; manifest pointer present; private state pointer points to the snapshot at supersede moment (read-only). |
| `RETIRED`            | A package object whose retention window expired or whose retention was waived by an operator. Tombstone only; binaries and probes purged. Manifest header retained for audit.            | Indefinite (audit row).                                   | `state = RETIRED`; manifest header present; binaries absent; private state absent.                                            |
| `QUARANTINED`        | A package object that violated a runtime invariant (capability lie, content tamper detected at load, sandbox breach, deplatform event) and is held in inert state pending operator.      | Indefinite until operator action.                         | `state = QUARANTINED`; binaries present but read-execute denied; private state preserved read-only.                           |
| `DRAFT`              | A package object created by the install pipeline before signature verification completes. Visible only to the install pipeline; never loadable by runtimes.                              | Window of the install pipeline; max 30 minutes.           | `state = DRAFT`; manifest pointer present (unverified); no probes; no private state.                                          |
| `VERIFICATION_PROBE` | A standalone probe object owned by S2.4 verification grammar, NOT a package per se but stored in the same shape because it shares the layout. Triggered on demand, not auto-loaded.      | Persistent until the parent package is uninstalled.       | `state ∈ {INSTALLED, ACTIVE}`; manifest pointer is the probe-bundle manifest; binaries are probe binaries; no private state.  |
| `EVIDENCE_BUNDLE`    | A standalone evidence-bundle object — upstream attestations (SBOM, third-party audit reports, build provenance) shipped by the publisher. Not executable. Read-only after install.       | Persistent until parent is uninstalled or retired.        | `state = INSTALLED`; manifest pointer is the evidence-bundle manifest; binaries absent; the bundle file present; immutable.   |

The eight kinds are exhaustive. A package object found on disk that does not match exactly one kind is treated as corruption: it is moved to `QUARANTINED` and `PACKAGE_OBJECT_LAYOUT_CORRUPTION` evidence is emitted (FOREVER).

### §3.2 `PackageContentKind`

Closed enum, ten values. Every file inside a package object directory is classified by the closed `PackageContentKind`. The classification is computed at install time and recorded in the package object's `meta.aios` file. Files of an unrecognized kind cause the install to fail with `BUNDLE_TAMPERED` (S11.1 §3.7).

| Value                        | Semantics                                                                                                         | Hash policy                          | Mutation policy                                                                                                                              |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------- | ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `CODE_BINARIES`              | ELF / Mach-O / PE / WASM / interpreter scripts — the executable artifacts of the package.                         | `BLAKE3` per file; aggregate Merkle. | Read-execute only; never writable post-install. Mutation triggers `PACKAGE_OBJECT_VERIFICATION_FAILED`.                                      |
| `DATA_ASSETS`                | Read-only data shipped by the publisher: locale bundles, fonts, images, model weights, sample data, schema files. | `BLAKE3` per file; aggregate Merkle. | Read-only; never writable post-install. Mutation triggers `PACKAGE_OBJECT_VERIFICATION_FAILED`.                                              |
| `CONFIGURATION`              | Default configuration files shipped by the publisher (e.g. an `etc/` directory inside the package). Read-only.    | `BLAKE3` per file; aggregate Merkle. | Read-only at the package layer. Per-instance configuration writes go to `PRIVATE_STATE_DIR`; never to `CONFIGURATION`.                       |
| `PRIVATE_STATE_DIR`          | The single writable subdirectory the package may use at runtime. Mounted under the package's sandbox.             | None at install (empty directory).   | Read-write only by the package's bound subject (§7). Cross-package access is forbidden by S2.3 (INV-011 extension). Per-load layout audited. |
| `VERIFICATION_PROBES`        | Probe binaries / probe bundles shipped by the publisher (or AIOS-curated for the package kind) used by S2.4.      | `BLAKE3` per file; aggregate Merkle. | Read-execute only; never writable post-install. Probe execution is sandboxed identically to the package itself.                              |
| `ROLLBACK_POINTERS`          | A small JSON manifest listing prior versions of this package id and their AIOS-FS object addresses (§10.3).       | `BLAKE3`; updated only on promotion. | Append-on-promotion only. Direct mutation forbidden; only the promotion engine writes.                                                       |
| `EVIDENCE_BUNDLE_REF`        | A pointer (BLAKE3 address) to the upstream `EVIDENCE_BUNDLE` object if any. Optional.                             | `BLAKE3`; immutable.                 | Read-only.                                                                                                                                   |
| `SANDBOX_PROFILE`            | The composed `SandboxProfile` artifact (frozen output of S3.2 §5 composition for this package at this version).   | `BLAKE3`; immutable per version.     | Read-only. New version = new composition = new `SANDBOX_PROFILE` artifact. Never edited in place.                                            |
| `NETWORK_OUTBOUND_MANIFEST`  | The frozen `NetworkOutboundManifest` (S8.1) for this package at this version.                                     | `BLAKE3`; immutable per version.     | Read-only. Same versioning policy as `SANDBOX_PROFILE`.                                                                                      |
| `DECLARED_CAPABILITIES_LIST` | The frozen list of capabilities the manifest declared, as parsed by the S11.1 install pipeline.                   | `BLAKE3`; immutable per version.     | Read-only. Drift between this list and runtime observation triggers `CAPABILITY_LIE` per S11.1 §7.                                           |

The ten kinds are exhaustive. Adding a kind requires a versioned spec change; the loader rejects any unrecognized file at parse time.

### §3.3 `PackageObjectState`

Closed enum, eight values. The state of a package object on disk **after** the install pipeline completes; distinct from S11.1's `PackageInstallState`, which is the state of an _in-flight install action_. A package object is created by the install pipeline only on the `INSTALLED` transition (S11.1 §3.6 `INSTALLING → ACTIVE`). Before that, no object exists on disk under `/aios/.../apps/`.

| Value         | Semantics                                                                                                                                                                                 | Loadable by runtime | Mutable | Visible to L7 marketplace                         |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------- | ------- | ------------------------------------------------- |
| `DRAFT`       | Created by the install pipeline before content-hash check completes. Window only; not exposed.                                                                                            | No                  | No      | No                                                |
| `INSTALLED`   | Object on disk; verification probes passed; private state initialized. Not yet started by the runtime. (Equivalent to "installed but never run".)                                         | Yes                 | No      | Yes (badge: "Installed; not yet run")             |
| `STAGED`      | A `STAGED_UPDATE` for an already-installed package. Held next to the `ACTIVE` peer until promotion or rejection.                                                                          | No                  | No      | Yes (badge: "Update available; verifying")        |
| `ACTIVE`      | Currently running or available to be launched. The default state for normal operation.                                                                                                    | Yes                 | No      | Yes (default)                                     |
| `SUPERSEDED`  | Was `ACTIVE`; replaced by a newer version through promotion. Retained for rollback within thirty calendar days.                                                                           | No                  | No      | No (visible only to rollback engine and operator) |
| `ROLLED_BACK` | Was `ACTIVE`; rolled back; the prior `SUPERSEDED` peer is now `ACTIVE` and this object is the rolled-back tombstone. Retained for further forensic analysis until uninstalled or expired. | No                  | No      | Yes (badge: "Rolled back")                        |
| `QUARANTINED` | Held inert pending operator review (capability lie, content tamper, deplatform).                                                                                                          | No                  | No      | Yes (badge: "Quarantined; review required")       |
| `RETIRED`     | Tombstone; binaries purged; only manifest header retained for audit. Terminal state for `SUPERSEDED → RETIRED` transition past the thirty-day window.                                     | No                  | No      | Yes (badge: "Retired")                            |

Allowed transitions:

```text
DRAFT ─▶ INSTALLED ──▶ ACTIVE ┬─▶ SUPERSEDED ──▶ RETIRED          (post-30d auto-retire)
                              │                  │
                              │                  └─▶ ACTIVE        (rollback within window: see §10.2 promotion-of-prior)
                              │
                              ├─▶ ROLLED_BACK                       (this object was rolled back; the SUPERSEDED peer becomes ACTIVE)
                              │
                              ├─▶ QUARANTINED                       (lie / tamper / deplatform / breach)
                              │
                              └─▶ RETIRED                           (operator uninstall; terminal)

STAGED ─┬─▶ ACTIVE          (promotion; concurrent peer transitions ACTIVE → SUPERSEDED)
        └─▶ RETIRED        (staged update rejected: pipeline failure, operator denial, manifest forged)

QUARANTINED ─┬─▶ RETIRED                                            (operator uninstall; terminal)
             └─▶ ACTIVE                                              (recovery-mode RECOVERY_RESTORE only; §10.5)
```

Forbidden transitions:

- `RETIRED → anything` — terminal.
- `ACTIVE → DRAFT` — never.
- `ACTIVE → STAGED` — never. A new staged update is a separate object.
- `STAGED → STAGED` — only one staged peer per package id at a time. A new staged arrival replaces the previous.
- `INSTALLED → SUPERSEDED` directly — must transition through `ACTIVE`. Otherwise the rollback graph would lose the never-run leaf.

The `PackageObjectState` of an object is recorded in the object's `state.aios` file. The state file is written by the package object engine; it is the only field in the layout permitted to mutate after install (modulo `PRIVATE_STATE_DIR`).

### §3.4 `RollbackKind`

Closed enum, four values. Records the rollback policy declared by the publisher at install time (in `PackageManifest`). The runtime rollback engine consults this enum to decide which rollback operations are admissible for the package.

| Value           | Semantics                                                                                                                                                                                          | Approval class for rollback                |
| --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| `NEVER`         | The publisher declared this package is never rollback-safe. Rollback requests are rejected with `RollbackForbiddenByPublisher`. Used for kernel candidates whose only remediation is forward-roll. | N/A                                        |
| `SINGLE_STEP`   | Rollback is supported only to the immediately-prior `SUPERSEDED` peer. Rollback further back is forbidden.                                                                                         | Operator approval (S5.3 EXACT_ACTION)      |
| `MULTI_VERSION` | Rollback is supported to any `SUPERSEDED` peer within the thirty-day window, subject to the rollback blocklist (§10.4). Default for app and adapter kinds.                                         | Operator approval (S5.3 EXACT_ACTION)      |
| `RECOVERY_ONLY` | Rollback is supported only under recovery mode. Used for `IDENTITY_BUNDLE`, `INVARIANT_BUNDLE`, `POLICY_BUNDLE`, `KERNEL_CANDIDATE`, `CAPABILITY_CATALOG_DELTA`. Binds INV-004 recovery boundary.  | Recovery operator + cosign (S5.3 RECOVERY) |

The kind is recorded in `PackageManifest` (S11.1 §4) and copied to `meta.aios` at install time. It is immutable for a given installed object. A new version may declare a different `RollbackKind`, but the new version's value applies only from its own promotion onward.

## §4 On-disk layout (closed)

This section fixes the closed table of files inside a package object directory. The layout is the **only** legal shape; the install pipeline produces it; the loader requires it. Any deviation is `PACKAGE_OBJECT_LAYOUT_CORRUPTION`.

### §4.1 Path templates

Three install-scope path templates are recognized; they map directly to S11.1 `InstallScope` and S4.1 namespace constraints:

| `InstallScope` | Path template                                                                            | S4.1 owner                      | INV bound        |
| -------------- | ---------------------------------------------------------------------------------------- | ------------------------------- | ---------------- |
| `SYSTEM_ONLY`  | `/aios/system/apps/<package_id>/`                                                        | recovery + system_admin (human) | INV-004, INV-012 |
| `GROUP_SCOPED` | `/aios/groups/<group_id>/apps/<package_id>/`                                             | group admin                     | INV-011          |
| `USER_SCOPED`  | `/aios/groups/<group_id>/users/<user_id>/apps/<package_id>/` (queued S4.1 touch-up; §13) | user owner                      | INV-011          |

The user-scope template requires an extension to S4.1 §6 `UserReservedName` enum (a new value `USR_APPS = 9`); the touch-up is queued for the next refinement wave (§13.1). Until S4.1 is updated, user-scoped installs are deferred and the install pipeline (S11.1 §5 step 6) rejects them with `InstallScopeViolation` carrying sub-reason `UserScopeAppsNotYetEnabled`.

A package object MAY exist at exactly one of the three scopes for a given `package_id`. Multi-scope coexistence is forbidden by S4.1 §10 (uniqueness of `app_id` across system and group scopes); this contract extends the constraint to user scope when enabled.

### §4.2 Required files (closed)

Every package object directory MUST contain exactly the following files (call this **the closed file set**):

| File                  | `PackageContentKind`         | Required for kinds                                                                                                                                       | Purpose                                                                                                                                                         |
| --------------------- | ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `meta.aios`           | (n/a — engine-owned)         | All except `RETIRED`                                                                                                                                     | The package object header: `package_id`, `version`, `kind`, `installed_at`, `installer_action_id`, `manifest_pointer`, `merkle_root`, `signature_envelope_ref`. |
| `state.aios`          | (n/a — engine-owned)         | All                                                                                                                                                      | The single mutable file: current `PackageObjectState` + `state_changed_at` + `last_state_reason`. Append-only history; rotates on retire.                       |
| `manifest.json`       | (manifest pointer; §6)       | All except `RETIRED`                                                                                                                                     | The frozen S11.1 `PackageManifest` for this version. Content-addressed; never re-written.                                                                       |
| `merkle.aios`         | (engine-owned)               | All except `RETIRED`                                                                                                                                     | The Merkle tree over `CODE_BINARIES + DATA_ASSETS + CONFIGURATION + VERIFICATION_PROBES`. The root matches `meta.aios.merkle_root`.                             |
| `code/` (directory)   | `CODE_BINARIES`              | `INSTALLED_PACKAGE`, `STAGED_UPDATE`, `ROLLBACK_RESERVE`, `QUARANTINED`, `VERIFICATION_PROBE`                                                            | The executable artifacts.                                                                                                                                       |
| `data/` (directory)   | `DATA_ASSETS`                | `INSTALLED_PACKAGE`, `STAGED_UPDATE`, `ROLLBACK_RESERVE`, `QUARANTINED`, `EVIDENCE_BUNDLE`                                                               | Read-only data assets.                                                                                                                                          |
| `config/` (directory) | `CONFIGURATION`              | `INSTALLED_PACKAGE`, `STAGED_UPDATE`, `ROLLBACK_RESERVE`, `QUARANTINED`                                                                                  | Default configuration files shipped by the publisher.                                                                                                           |
| `state/` (directory)  | `PRIVATE_STATE_DIR`          | `INSTALLED_PACKAGE` (only when `state ∈ {INSTALLED, ACTIVE, ROLLED_BACK}`); `ROLLBACK_RESERVE` (read-only snapshot); `QUARANTINED` (preserved read-only) | The package's writable private state. See §7.                                                                                                                   |
| `probes/` (directory) | `VERIFICATION_PROBES`        | `INSTALLED_PACKAGE`, `STAGED_UPDATE`, `ROLLBACK_RESERVE`, `QUARANTINED`, `VERIFICATION_PROBE`                                                            | Probe binaries used by S2.4.                                                                                                                                    |
| `rollback.json`       | `ROLLBACK_POINTERS`          | `INSTALLED_PACKAGE`, `ROLLBACK_RESERVE`                                                                                                                  | Pointers to prior `SUPERSEDED` peers and their `merkle_root`s.                                                                                                  |
| `sandbox.json`        | `SANDBOX_PROFILE`            | All loadable kinds                                                                                                                                       | The composed `SandboxProfile` artifact.                                                                                                                         |
| `network.json`        | `NETWORK_OUTBOUND_MANIFEST`  | All loadable kinds                                                                                                                                       | The frozen `NetworkOutboundManifest`.                                                                                                                           |
| `capabilities.json`   | `DECLARED_CAPABILITIES_LIST` | All loadable kinds                                                                                                                                       | The frozen declared-capabilities list.                                                                                                                          |

Optional files (closed list):

| File               | `PackageContentKind`     | Required when                                | Purpose                                                                              |
| ------------------ | ------------------------ | -------------------------------------------- | ------------------------------------------------------------------------------------ |
| `evidence.json`    | `EVIDENCE_BUNDLE_REF`    | If publisher shipped an evidence bundle      | Pointer to the publisher-shipped `EVIDENCE_BUNDLE` (BLAKE3 address; immutable).      |
| `unit.aios`        | (manifest pointer; §6.4) | If `kind = SERVICE` (queued S15.1)           | Pointer to the S15.1 unit manifest object. Existence-check only; opaque to S12.2.    |
| `recipe.json`      | (manifest pointer; §6.5) | If installed via S12.1 community recipe      | Pointer to the S12.1 recipe object that produced this package's manifest proposal.   |
| `ecosystem.json`   | (manifest pointer; §6.6) | If `EcosystemRuntime ≠ RUNTIME_LINUX_NATIVE` | Pointer to the S12.1 `EcosystemRuntime` adapter package binding for this package.    |
| `attestation.aios` | (engine-owned)           | If install ran in recovery mode              | A signed receipt that the install action was approved under recovery; FOREVER trail. |
| `tombstone.aios`   | (engine-owned)           | When `state = RETIRED`                       | The retire receipt; replaces all other artifacts in `RETIRED` kind.                  |

The closed file set is exhaustive. A loader that encounters an unrecognized file inside a package object directory rejects the whole object with `PACKAGE_OBJECT_LAYOUT_CORRUPTION` and emits FOREVER `PACKAGE_OBJECT_LAYOUT_CORRUPTION` evidence (§11). No "extra files" allowed; the directory is canonical.

### §4.3 Per-kind required-file matrix (closed)

The matrix below is the **definitive truth** of which files are required for which `PackageObjectKind`. The install pipeline produces these files; the loader requires them; the `meta.aios` engine cross-checks each entry against this matrix at every load.

| File                | `INSTALLED_PACKAGE`            | `STAGED_UPDATE` | `ROLLBACK_RESERVE`     | `RETIRED` | `QUARANTINED`           | `DRAFT`               | `VERIFICATION_PROBE`             | `EVIDENCE_BUNDLE`                   |
| ------------------- | ------------------------------ | --------------- | ---------------------- | --------- | ----------------------- | --------------------- | -------------------------------- | ----------------------------------- |
| `meta.aios`         | required                       | required        | required               | required  | required                | required              | required                         | required                            |
| `state.aios`        | required                       | required        | required               | required  | required                | required              | required                         | required                            |
| `manifest.json`     | required                       | required        | required               | absent    | required                | required (unverified) | required (probe-bundle manifest) | required (evidence-bundle manifest) |
| `merkle.aios`       | required                       | required        | required               | absent    | required                | absent (incomplete)   | required                         | required                            |
| `code/`             | required                       | required        | required               | absent    | required (RX-denied)    | absent                | required                         | absent                              |
| `data/`             | required                       | required        | required               | absent    | required (R-denied)     | absent                | absent                           | required                            |
| `config/`           | required                       | required        | required               | absent    | required (R-denied)     | absent                | absent                           | absent                              |
| `state/`            | required                       | absent          | required (RO snapshot) | absent    | required (RO preserved) | absent                | absent                           | absent                              |
| `probes/`           | required                       | required        | required               | absent    | required (RX-denied)    | absent                | required                         | absent                              |
| `rollback.json`     | required                       | absent          | required               | absent    | required                | absent                | absent                           | absent                              |
| `sandbox.json`      | required                       | required        | required               | absent    | required                | absent                | required                         | absent                              |
| `network.json`      | required                       | required        | required               | absent    | required                | absent                | required                         | absent                              |
| `capabilities.json` | required                       | required        | required               | absent    | required                | absent                | required                         | absent                              |
| `evidence.json`     | optional                       | optional        | optional               | absent    | optional                | absent                | absent                           | required (self)                     |
| `unit.aios`         | conditional (service)          | conditional     | conditional            | absent    | conditional             | absent                | absent                           | absent                              |
| `recipe.json`       | conditional (recipe-installed) | conditional     | conditional            | absent    | conditional             | absent                | absent                           | absent                              |
| `ecosystem.json`    | conditional (non-Linux-native) | conditional     | conditional            | absent    | conditional             | absent                | absent                           | absent                              |
| `attestation.aios`  | conditional (recovery install) | conditional     | conditional            | absent    | conditional             | absent                | absent                           | absent                              |
| `tombstone.aios`    | absent                         | absent          | absent                 | required  | absent                  | absent                | absent                           | absent                              |

"Required (RX-denied)" = the file is required to be present on disk for forensic and rollback purposes, but read-execute access is denied to all subjects except the recovery operator. "Required (R-denied)" = read access denied. "Required (RO preserved)" = read-only preserved; not deleted; not mutated.

## §5 On-disk layout — examples

### §5.1 System-scope app

```text
/aios/system/apps/evidence-viewer/
├── meta.aios                              # PackageObjectKind=INSTALLED_PACKAGE, state=ACTIVE, version=2.4.1
├── state.aios                             # current state file (single mutable artifact)
├── manifest.json                          # frozen PackageManifest (S11.1 §4)
├── merkle.aios                            # Merkle root over code+data+config+probes
├── code/
│   └── evidence-viewer                    # ELF binary
├── data/
│   ├── locale/                            # i18n bundles
│   └── icons/                             # icon assets
├── config/
│   └── default.toml                       # publisher-shipped defaults
├── state/                                 # PRIVATE_STATE_DIR (sandbox-restricted)
│   └── viewer-prefs.aios
├── probes/
│   └── invariants.probe                   # S2.4 probe bundle
├── rollback.json                          # pointers to prior versions (2.4.0, 2.3.x)
├── sandbox.json                           # composed SandboxProfile
├── network.json                           # NetworkOutboundManifest (system-only services)
├── capabilities.json                      # declared capabilities (system_audit_read, evidence_query)
└── attestation.aios                       # install was performed under recovery mode (system_only requires recovery)
```

### §5.2 Group-scope app installed via Proton (Steam game)

```text
/aios/groups/family/apps/factorio/
├── meta.aios                              # PackageObjectKind=INSTALLED_PACKAGE, state=ACTIVE, version=1.1.106
├── state.aios
├── manifest.json
├── merkle.aios
├── code/
│   └── factorio.exe                       # PE binary
├── data/
│   ├── data/                              # game assets
│   └── locale/
├── config/
│   └── config.ini                         # publisher defaults
├── state/                                 # private state — saves, mod data
│   ├── saves/
│   └── mods/
├── probes/
│   └── factorio.probe                     # invariant probes
├── rollback.json                          # prior versions: 1.1.105, 1.1.104
├── sandbox.json                           # composed SandboxProfile (Proton-runtime floor + app additions)
├── network.json                           # NetworkOutboundManifest (Steam multiplayer, mod portal)
├── capabilities.json                      # declared capabilities (gamepad, audio, gpu_compute_low)
├── ecosystem.json                         # EcosystemRuntime=RUNTIME_WINDOWS_PROTON binding
└── recipe.json                            # community recipe pointer (S12.1)
```

### §5.3 Group-scope service install (queued S15.1)

```text
/aios/groups/family/apps/home-photos-service/
├── meta.aios                              # PackageObjectKind=INSTALLED_PACKAGE, state=ACTIVE, version=0.9.2
├── state.aios
├── manifest.json
├── merkle.aios
├── code/
│   └── home-photos-server                 # ELF service binary
├── data/
│   └── migrations/                        # SQLite schema migrations
├── config/
│   └── server.toml                        # publisher defaults
├── state/                                 # private state — DB, thumbnails
│   ├── photos.db
│   └── thumbnails/
├── probes/
│   └── service.probe
├── rollback.json
├── sandbox.json
├── network.json                           # NetworkOutboundManifest (LAN-only)
├── capabilities.json
├── unit.aios                              # ← S15.1 unit manifest pointer (queued; opaque to S12.2)
└── ecosystem.json                         # EcosystemRuntime=RUNTIME_LINUX_NATIVE
```

### §5.4 Staged update peer

```text
/aios/groups/family/apps/factorio/
├── ... (the ACTIVE peer, version 1.1.106 — see §5.2)
└── _staged/                               # ← staged update peer subdirectory (engine-reserved name)
    ├── meta.aios                          # PackageObjectKind=STAGED_UPDATE, state=STAGED, version=1.1.107
    ├── state.aios
    ├── manifest.json                      # new version's manifest (different content_hash)
    ├── merkle.aios
    ├── code/                              # new version binaries
    ├── data/                              # new version data
    ├── config/                            # new version defaults
    ├── probes/                            # new version probes
    ├── sandbox.json
    ├── network.json
    └── capabilities.json
```

The `_staged/` subdirectory is a reserved engine-only name; subjects cannot create it. The package object loader recognizes `_staged/` as a sibling staged peer; promotion (§10.2) atomically renames `_staged/` to the new active layout while moving the prior active to `_rollback_<n>/`.

### §5.5 Rollback reserve peer

```text
/aios/groups/family/apps/factorio/
├── ... (ACTIVE peer)
├── _rollback_1/                           # most recent SUPERSEDED peer (RollbackKind=MULTI_VERSION)
│   ├── meta.aios                          # PackageObjectKind=ROLLBACK_RESERVE, state=SUPERSEDED, version=1.1.106
│   ├── state.aios
│   ├── manifest.json
│   ├── merkle.aios
│   ├── code/
│   ├── data/
│   ├── config/
│   ├── state/                             # snapshot of private state at supersede moment (read-only)
│   ├── probes/
│   ├── rollback.json
│   ├── sandbox.json
│   ├── network.json
│   └── capabilities.json
└── _rollback_2/                           # next-older peer
```

`_rollback_<n>/` is engine-reserved. `n=1` is the most-recently-superseded; higher `n` is older. The retention engine retires `_rollback_<n>/` thirty calendar days after its `meta.aios.installed_at` (i.e. when the peer was first installed as `INSTALLED_PACKAGE` before being superseded).

## §6 Manifest pointer discipline

The `meta.aios.manifest_pointer` field is a BLAKE3 address into AIOS-FS pointing at the frozen S11.1 `PackageManifest` blob. The pointer is content-addressed; it is **never** a relative path inside the package directory. The actual `manifest.json` file inside the directory is a materialization of the same content addressed by the pointer. Loader policy:

1. The loader reads `meta.aios.manifest_pointer`.
2. The loader computes `BLAKE3(manifest.json bytes)` and compares against the pointer.
3. Mismatch → `MANIFEST_MATERIALIZATION_DRIFT` (a sub-reason of `PACKAGE_OBJECT_LAYOUT_CORRUPTION`); object is `QUARANTINED`; FOREVER evidence.

This discipline ensures a malicious in-place edit of `manifest.json` cannot survive the next load.

### §6.1 Service unit manifest pointer (queued S15.1)

If the package's manifest declares `kind = SERVICE` (a value queued for S11.1 §3.4 future extension; until then, services are admitted via `kind = APP` with a `service_unit` extension flag in the manifest), the package directory MUST contain `unit.aios` carrying a BLAKE3 address into AIOS-FS pointing at the future S15.1 unit manifest object. S12.2 is intentionally **opaque** to the unit manifest content; it only checks that the pointer exists, that the hash resolves, and that the address is not zero. Future S15.1 will define unit-manifest semantics; this contract pins the pointer location.

### §6.2 Recipe pointer (S12.1)

If the install was triggered by a community recipe (S12.1 §6), `recipe.json` carries a BLAKE3 address into AIOS-FS pointing at the recipe object that produced the manifest proposal. The pointer is informational; loader does not re-resolve recipe semantics. Used by S9.1 forensic queries: "which recipe produced this install?"

### §6.3 Ecosystem runtime binding

If `EcosystemRuntime ≠ RUNTIME_LINUX_NATIVE`, `ecosystem.json` carries the BLAKE3 address of the S12.1 `EcosystemRuntime` adapter package object (an `ADAPTER` per S11.1 §3.4). The composed `SandboxProfile` in `sandbox.json` MUST be consistent with the adapter's declared profile floor; mismatch is a load-time integrity failure.

## §7 Private state directory contract

`PRIVATE_STATE_DIR` (`state/`) is the only writable subdirectory inside a package object. Every other subdirectory is read-only post-install (modulo controlled engine writes for `state.aios`, `merkle.aios`, `rollback.json`).

### §7.1 Binding to subject identity

The directory is bound to a single subject canonical id at install time (S4.1 §12.8 forms):

| Install scope  | Bound subject                                                                                                                   |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `SYSTEM_ONLY`  | `_system:<service_name>` derived from the manifest's `service_id` (or `_system:apps:<package_id>` for non-service system apps). |
| `GROUP_SCOPED` | `<group_id>:apps:<package_id>` — a synthetic group-app subject. Membership in the bound group is required to launch.            |
| `USER_SCOPED`  | `<group_id>:<user_id>:apps:<package_id>` (queued; pending S4.1 user-scope `apps/` enablement).                                  |

The binding is recorded in `meta.aios.bound_subject` and is immutable for the package object's lifetime. The S3.2 sandbox composition for the package writes a filesystem rule allowing read-write only when the running subject's canonical id matches `meta.aios.bound_subject`. Cross-subject read attempts are denied by default-deny (S2.3).

### §7.2 Cross-package access prohibition

A package's `PRIVATE_STATE_DIR` is **never** accessible from another package, even within the same group, even when the other package is held by the same user. The S3.2 composed `SandboxProfile` materializes a filesystem rule of the form:

```text
filesystem:
  allow:
    - path: /aios/groups/<group_id>/apps/<package_id>/state/
      mode: read_write
      subject_match: "<group_id>:apps:<package_id>"
  deny_default: true
```

Any cross-package access attempt (e.g. package `A` reading `/aios/groups/<g>/apps/<B>/state/`) is denied by S2.3 with `CrossGroupAccessForbidden` (extended by sub-reason `CrossPackageStateAccessForbidden`) and emits FOREVER `PACKAGE_PRIVATE_STATE_CROSS_PACKAGE_ACCESS_DENIED` evidence (a sub-record of `CROSS_GROUP_ACCESS_DENIED` per S4.1 §12.6 / S3.1).

This binds INV-011 (cross-group access forbidden by default): packages are strictly subordinate to the group/user trust unit and may not lateral-move into peers' state.

### §7.3 Initialization

On `state = INSTALLED → ACTIVE` transition (the very first launch), the package object engine:

1. Creates `state/` if absent (atomically; AIOS-FS optimistic concurrency).
2. Applies the installer's declared `state_init` (a per-package optional `state_init.json` shipped under `data/state_init/` and copied into `state/` once at first launch). State-init files are read-only inside `data/`; copies into `state/` become writable.
3. Records `state.aios.private_state_initialized_at`.
4. Emits `PACKAGE_PRIVATE_STATE_INITIALIZED` (`STANDARD_24M`).

### §7.4 Corruption detection

On every load (every launch, every probe run), the package object engine recomputes a **layout checksum** over `state/` (a BLAKE3 over the directory listing — file names + sizes + content hashes for a closed manifest of "audited" state subpaths declared by the publisher in `state_init.json`). If the publisher declared no audited subpaths, the layout checksum is over the directory listing only.

If the layout checksum has been modified by any process not bound to the package's subject (detectable by S3.1 evidence cross-reference), `PACKAGE_PRIVATE_STATE_CORRUPT_DETECTED` is emitted (FOREVER); the package object transitions to `QUARANTINED`; the operator is notified.

Note: this is **not** a guarantee that the package's own writes are correct. The contract detects unauthorized writes from outside the package's subject identity, not application-level corruption.

### §7.5 Snapshot on supersede

When a package is superseded (§10.2), the engine takes an atomic snapshot of `state/` and stores it under `_rollback_<n>/state/` as read-only. The snapshot is content-addressed via AIOS-FS S1.3; it shares chunks with the original where unchanged. Rollback restores the snapshot atomically (§10.3).

### §7.6 Quarantine state preservation

When a package transitions to `QUARANTINED`, `state/` is preserved read-only (not deleted) for forensic analysis. A subsequent operator decision (uninstall vs. recovery-restore) determines the state's fate.

## §8 Versioning (consumes S1.3)

Every file inside a package object directory is itself an AIOS-FS object per S1.3 — content-addressed, immutable, versioned. The package directory is a top-level AIOS-FS object whose pointer enumerates child pointers. Mutation of any child (including the engine-owned `state.aios` and the package's own `state/` writes) bumps the parent directory's version through S1.3 optimistic concurrency.

### §8.1 Why versioning matters here

Three semantics fall out of S1.3:

1. **Atomic rollback**: a `_rollback_<n>/` peer points at the AIOS-FS object version of the prior package directory. Restoring it is a constant-time pointer swap, not a copy.
2. **Tamper detection**: any file edit produces a new content-hash and bumps the parent directory's version. The loader checks `meta.aios.version_at_install` against the current AIOS-FS version of the directory; drift is a tamper signal.
3. **Concurrent installs**: two install pipelines targeting the same `package_id` resolve via S1.3 conflict detection; one wins, the other receives `ConflictDetected` from S1.3 and the install pipeline (S11.1 §5) re-enters validation.

### §8.2 Version recording

`meta.aios` carries:

- `package_version` — the publisher-declared semantic version (e.g. `1.1.106`). Sourced from `PackageManifest.version`.
- `aiosfs_object_version` — the AIOS-FS S1.3 version of the package directory at install completion.
- `installed_at` — wall-clock timestamp.
- `installer_action_id` — the S0.1 `ActionId` of the install action envelope.

The loader treats `package_version` as informational (publisher claim). The loader treats `aiosfs_object_version` as authoritative for tamper detection. Drift between `package_version` (claimed) and the actual version implied by the directory's content is also a tamper signal.

### §8.3 Sibling peer versioning

Sibling `_staged/` and `_rollback_<n>/` peers each have their own AIOS-FS object version. The parent `<package_id>/` directory's version is the union of its children's pointers, so any peer mutation bumps the parent. This makes "did anything in this package's installation tree change?" a single S1.3 version check.

## §9 Per-load content-hash verification

Every load of a package object — every launch, every probe run, every metadata read by the L7 marketplace surface — recomputes the Merkle root over the directory's static content (`code/ + data/ + config/ + probes/`) and compares against `meta.aios.merkle_root`.

### §9.1 Algorithm

```text
1. Load meta.aios; read merkle_root_at_install.
2. Recompute Merkle over code/ + data/ + config/ + probes/ using BLAKE3 leaf, BLAKE3 inner.
3. Compare recomputed root to merkle_root_at_install.
   - Equal → load proceeds.
   - Not equal → tamper detected.
4. On tamper:
   - Transition state to QUARANTINED (write state.aios).
   - Emit PACKAGE_OBJECT_VERIFICATION_FAILED (EXTENDED_60M).
   - Refuse the load with PackageObjectIntegrityFailed.
   - Notify operator via L7 inbox (system or group).
```

The Merkle deliberately excludes `state/` (per-package mutable), `state.aios` (engine-mutable), `merkle.aios` (the digest itself), `rollback.json` (engine-updated on supersede), and the optional pointer files (`unit.aios`, `recipe.json`, `ecosystem.json`, `attestation.aios`, `tombstone.aios` if present). These are tracked separately:

- `state.aios` is a single-line append log; tamper is detected by absence of the latest known transition entry.
- `rollback.json` carries its own BLAKE3 in `meta.aios.rollback_json_hash_at_last_promotion`; tamper since last promotion is detected.

### §9.2 Caching

For performance, the loader may cache a successful Merkle verification per `(aiosfs_object_version, current_time)`. Cache TTL is at most sixty seconds; cache eviction on any S1.3 version change of the directory.

### §9.3 Probe-driven re-verification

S2.4 verification grammar may schedule property `PACKAGE_OBJECT_LAYOUT_INTACT` (queued at §13.2). When scheduled, S2.4 invokes the package's `probes/` runtime and forces a full Merkle recompute regardless of cache. Probe-driven re-verification is the guaranteed way to clear a stale cache.

## §10 Update flow and rollback

This section is the dynamic core of S12.2. It describes how a package transitions from a single-version install to a multi-version history, how that history is bounded, and how rollback consumes it.

### §10.1 Initial install

Install pipeline (S11.1 §5) completes with the package's `PackageInstallState = ACTIVE`. The package object engine:

1. Creates `/aios/.../apps/<package_id>/` (with closed file set).
2. Sets `meta.aios.kind = INSTALLED_PACKAGE`, `state.aios.state = INSTALLED`.
3. Triggers first-launch transition `INSTALLED → ACTIVE` on the operator's first launch action (or auto-launch for services per queued S15.1).
4. Emits `PACKAGE_OBJECT_CREATED` (`STANDARD_24M`).

There is no `_rollback_<n>/` peer at this point. `rollback.json` contains an empty `prior_versions` array.

### §10.2 Staged update and promotion

When a new version of an already-installed package arrives through the install pipeline:

1. The pipeline allocates `<package_id>/_staged/` and writes the new layout into it.
2. The pipeline runs S11.1 verification against the staged peer (signature, chain, content-hash, capability declaration).
3. On verification success, `_staged/` carries `state = STAGED`.
4. **Promotion** is a separate action envelope (S0.1) routed through S2.3 policy — never auto-applied. Approval class:
   - `STABLE` channel: operator EXACT_ACTION approval.
   - `BETA` channel: operator EXACT_ACTION approval (must be opt-in per package per S11.1 §3.3).
   - `RECOVERY_CRITICAL` channel: recovery-mode + cosign.
5. On promotion approval, the engine performs an atomic three-step rename within S1.3:

   ```text
   <package_id>/         → <package_id>/_rollback_1/   (prior active becomes rollback_1)
   <package_id>/_staged/ → <package_id>/               (staged becomes active)
   _rollback_<n>/        → _rollback_<n+1>/            (existing rollback peers shift)
   ```

   The rename is atomic at the AIOS-FS S1.3 level (single pointer swap on the parent directory).

6. The new active package's `state.aios` transitions through `STAGED → INSTALLED → ACTIVE` (a single launch step). The prior active's `state.aios` is updated in-place to `ACTIVE → SUPERSEDED`.
7. Private state: the new active inherits the prior active's `state/` snapshot **only if** the staged manifest declared `state_inheritance = TRUE` (a queued field in `PackageManifest`; until then the default is `TRUE` for matching major version, `FALSE` across major versions). Inheritance is a copy-on-write S1.3 pointer; it does not duplicate chunks.
8. The engine emits `PACKAGE_OBJECT_UPDATED` (`STANDARD_24M`) on the new active and `PACKAGE_OBJECT_SUPERSEDED` (`STANDARD_24M`) on the prior.

If verification of the staged peer fails, the engine retires `_staged/` (transitions to `RETIRED`, writes `tombstone.aios`, leaves the active peer untouched) and emits `PACKAGE_OBJECT_VERIFICATION_FAILED` (`EXTENDED_60M`).

### §10.3 Rollback flow

A rollback request is an S0.1 action envelope with target `<package_id>` and a parameter `rollback_to = <version>` (or `rollback_to = PRIOR` for single-step). The engine:

1. Validates against `meta.aios.rollback_kind`:
   - `NEVER` → reject with `RollbackForbiddenByPublisher`; FOREVER `PACKAGE_VERSION_ROLLBACK_FORBIDDEN` evidence.
   - `SINGLE_STEP` → only `rollback_to = PRIOR` accepted; multi-version requests rejected.
   - `MULTI_VERSION` → any `_rollback_<n>/` within the thirty-day window accepted.
   - `RECOVERY_ONLY` → `is_recovery_mode = true` required on the subject.
2. Consults the `RollbackBlocklist` (§10.4). If the target version is blocklisted, reject with `RollbackBlockedByCVE`; FOREVER `PACKAGE_VERSION_DOWNGRADE_BLOCKED` evidence.
3. Routes through S2.3 policy with the appropriate approval class (per `RollbackKind`).
4. On approval, performs an atomic three-step rename within S1.3:

   ```text
   <package_id>/                 → <package_id>/_rolled_back_<m>/    (current active becomes rollback tombstone with state=ROLLED_BACK)
   <package_id>/_rollback_<k>/   → <package_id>/                     (target rollback peer becomes active)
   ```

5. The newly active peer's `state.aios` transitions `SUPERSEDED → ACTIVE`. The peer that just rolled back is `ACTIVE → ROLLED_BACK`.
6. Private state: `state/` of the now-active peer is the snapshot taken at supersede time (read-only at storage layer; promoted to read-write on rollback by sandbox rule). Modern state changes since the supersede are **lost**; this is the rollback contract. The operator is shown a "state-loss preview" before approval, including byte counts and last-modified summaries. The engine never silently merges modern state across the rollback boundary.
7. The engine emits `PACKAGE_OBJECT_ROLLED_BACK` (FOREVER).

### §10.4 Rollback blocklist (CVE flagging)

The `RollbackBlocklist` is a content-addressed AIOS-FS object owned by S11.1 publisher reputation feed (S11.1 §10 — publisher key compromise + downgrade attack). The blocklist contains one entry per `(publisher_root_id, package_id, version)` tuple flagged as known-vulnerable. Every rollback target is consulted against the blocklist before approval is requested.

The blocklist is updated by:

- the publisher itself (publisher-signed flag, valid only for the publisher's own packages);
- AIOS root (for cross-publisher critical CVEs; cosigned takedown extension);
- recovery-mode operator action (an explicit local override; FOREVER evidence).

A rollback to a blocklisted version is **forbidden by default**; the operator can request an override under recovery mode only. The override emits FOREVER `PACKAGE_VERSION_DOWNGRADE_BLOCKED_OVERRIDE` evidence with the operator id, the CVE reference, and the explicit acknowledgment.

### §10.5 Recovery restore

A `QUARANTINED` package object can be returned to `ACTIVE` only via a `RECOVERY_RESTORE` action — recovery mode + cosign. The recovery operator selects a target `_rollback_<n>/` peer (any prior version, blocklist-permitting). The engine:

1. Verifies the target peer's Merkle (full re-verification, no cache).
2. Drops the quarantined active (transitions to `RETIRED`).
3. Promotes the target peer to `ACTIVE`.
4. Emits `PACKAGE_RECOVERY_RESTORE_PERFORMED` (FOREVER).

If no `_rollback_<n>/` peer is available within the thirty-day window, the package is uninstallable from this state — the operator must full-uninstall and re-install from the marketplace (S11.1 install pipeline runs from scratch).

### §10.6 Retention discipline

A `_rollback_<n>/` peer is retained for thirty calendar days from the moment of its supersede. The retention engine runs daily and:

1. For each `_rollback_<n>/` whose `state.aios.state_changed_at[SUPERSEDED]` is older than thirty days:
   - Verify no active rollback action references this peer (no in-flight envelope).
   - Transition `state = SUPERSEDED → RETIRED`.
   - Replace all artifacts with a single `tombstone.aios`.
   - Emit `PACKAGE_OBJECT_RETIRED` (`EXTENDED_60M`).
2. Update `rollback.json` of the active peer to remove the retired peer from `prior_versions`.

The thirty-day retention is a constitutional default. It cannot be shortened by any policy bundle. It can be **extended** by a per-package operator declaration (a `retention_extension` field on the install action envelope, capped at ninety days; subject to disk-space governance).

### §10.7 Operator-driven uninstall

An operator-driven uninstall transitions the active peer through `ACTIVE → UNINSTALLING → REMOVED` (mirrored from S11.1 §3.6 install-time FSM, but here it's the disk-side mirror). The engine:

1. Stops any running runtime processes bound to the package (cooperative SIGTERM with a deadline; SIGKILL on timeout).
2. Tears down sandbox bindings.
3. Snapshots `state/` if the operator requested an export (optional; written to `/aios/groups/<g>/users/<u>/exports/`).
4. Removes the active peer's directory.
5. **Retains** all `_rollback_<n>/` peers for the operator's grace period (default seven days; extendable). Allows "I uninstalled but want to re-install" without losing private state; new install can opt in to `state_inheritance` from the most recent rollback peer.
6. After the grace period, retires all rollback peers as well.
7. Emits `PACKAGE_OBJECT_RETIRED` (`EXTENDED_60M`) for the active peer; one per rollback peer at retention expiry.

## §11 Adversarial robustness

### §11.1 Package content tampering on disk

**Adversary.** A package has been installed legitimately through S11.1. Later, a process with filesystem access (a misconfigured backup tool, a bug in another package, a successful sandbox escape, or a hostile recovery-mode operation) writes new bytes into the package's `code/` or `data/`.

**Detection.** Per-load content-hash verification (§9). Any drift between recomputed Merkle and `meta.aios.merkle_root` triggers `PACKAGE_OBJECT_VERIFICATION_FAILED` (EXTENDED_60M), transitions the object to `QUARANTINED`, and refuses the load.

**Why the install-time check is insufficient.** Install-time verification proves the bytes were good at install moment. After install, the bytes live on the host's filesystem for the package's lifetime. Per-load re-verification is the only mechanism that catches post-install tampering. Cache TTL (§9.2) bounds the freshness of the result at sixty seconds.

**Why caching is acceptable.** A sixty-second window between actual tamper and detection is acceptable because (a) any state-changing operation through S2.3 policy bypasses the cache (the cache only short-circuits read-only metadata loads), and (b) S2.4 scheduled probes force a recompute.

**Why the Merkle excludes `state/`.** Application state changes legitimately and frequently. Including it would make per-load verification false-positive on every state mutation. The `state/` directory has its own integrity contract (§7.4) which detects writes from outside the package's bound subject.

### §11.2 Rollback to vulnerable version

**Adversary.** An attacker (or a confused operator) requests rollback to a version of the package known to contain a critical CVE. The attack works either as a direct downgrade attack (after compromising the operator's UI session) or as social engineering ("the new version doesn't work; please roll back").

**Detection.** §10.4 `RollbackBlocklist`. Every rollback request consults the blocklist before approval is requested. A blocklisted target is rejected by default. An override is possible only under recovery mode with FOREVER evidence.

**Why publisher-only flagging is insufficient.** A compromised publisher might fail to flag its own vulnerable versions. The `RollbackBlocklist` therefore admits AIOS-root entries (cross-publisher CVEs) and recovery-mode entries (local operator override of unflagged version).

**Why a cap on rollback depth.** The thirty-day retention window also caps how far back an attacker can roll. After thirty days, prior versions are retired and unreachable; the operator must full-uninstall and re-install. This bounds the attack surface in time.

### §11.3 Private state poisoning

**Adversary.** A process not bound to the package's subject identity (a peer package, a misconfigured backup, a recovery-mode operation) writes to `/aios/.../apps/<package_id>/state/`. The attacker hopes the package's next launch consumes the poisoned data.

**Detection.** §7.4 layout checksum at load. The package object engine cross-references the most recent S3.1 evidence stream for any write activity to `state/` from a subject not equal to `meta.aios.bound_subject`. A mismatch transitions the package to `QUARANTINED` with FOREVER `PACKAGE_PRIVATE_STATE_CORRUPT_DETECTED` evidence.

**Why sandbox prevention is the primary defense.** The S3.2 composed `SandboxProfile` denies cross-subject writes by default (§7.2). Detection is a fallback for sandbox bugs, recovery-mode mistakes, and pre-S3.2 bootstrap windows.

**Why the audit list is closed.** If the publisher declared no audited subpaths in `state_init.json`, the layout checksum is over the directory listing only (file names + sizes). This catches the gross "extra file appeared" case but misses subtle in-place edits to declared-volatile data. A publisher that wants stronger detection declares specific subpaths as audited; those are then content-hashed at every load.

### §11.4 Staged-update promotion abuse

**Adversary.** An attacker plants files into `<package_id>/_staged/` directly (bypassing the install pipeline) and asks for promotion.

**Detection.** Promotion (§10.2) requires an S0.1 action envelope routed through S2.3 policy. The policy kernel demands proof that the staged peer was created by the install pipeline — specifically, an `installer_action_id` reference in `_staged/meta.aios` matching a successful S11.1 verification action. A fabricated `_staged/` without a valid action lineage fails policy with `StagedPeerLineageMissing` and emits `PACKAGE_OBJECT_VERIFICATION_FAILED` (EXTENDED_60M).

**Defence in depth.** The install pipeline is the only authorized writer of `_staged/`. S3.2 sandbox composition denies write access to package subdirectories from any subject other than the install pipeline's subject (`_system:installer`). A direct write attempt is blocked by sandbox before the file lands.

### §11.5 Recovery-mode skew

**Adversary.** During recovery, the kernel needs to enumerate installed packages (e.g. to ask "which evidence viewer is on this host?") before L5 (cognitive) is reachable. If package enumeration depended on L5 (e.g. on AI-translated metadata), recovery would deadlock.

**Mitigation.** The on-disk layout (§4) is **mechanical**: a finite set of files with deterministic names. The package object loader is implemented in Rust at L1 and does not depend on L5. Enumeration of `/aios/system/apps/` and `/aios/groups/<g>/apps/` is a pure filesystem walk plus per-directory `meta.aios` parse. Recovery never asks the cognitive subsystem about a package.

This binds INV-004 (recovery boundary preserved): package enumeration is recovery-safe by construction.

### §11.6 Concurrent install / uninstall race

**Adversary.** Two operator sessions issue conflicting actions concurrently — install vs. uninstall, install vs. install of a different version, promotion vs. rollback.

**Mitigation.** Every package object mutation is an AIOS-FS S1.3 optimistic-concurrency operation on the package directory's pointer. The first action wins; the second receives `ConflictDetected` from S1.3 and is rejected by the package object engine with `PackageObjectConflictDetected`. The losing action's submitter receives a `request_approval` re-prompt with the current state surfaced.

### §11.7 Cross-package state read

**Adversary.** Package A's binary attempts to read `/aios/groups/<g>/apps/<B>/state/` directly via a `read()` syscall.

**Mitigation.** S3.2 composed `SandboxProfile` deny-default. The syscall is blocked by Landlock / seccomp before reaching AIOS-FS. Even if it reached AIOS-FS (e.g. through a sandbox bug), S2.3 policy would reject the action with `CrossGroupAccessForbidden` (sub-reason `CrossPackageStateAccessForbidden`).

**Evidence.** FOREVER `PACKAGE_PRIVATE_STATE_CROSS_PACKAGE_ACCESS_DENIED` (sub-record under S3.1's `CROSS_GROUP_ACCESS_DENIED`).

### §11.8 Layout corruption injection

**Adversary.** An attacker adds a file to a package object directory that the loader does not recognize (an "extra" file outside the closed file set).

**Mitigation.** The closed file set (§4.2) is exhaustive. The loader rejects the whole package object on the first unrecognized file. No "extras" are tolerated. FOREVER `PACKAGE_OBJECT_LAYOUT_CORRUPTION` evidence.

**Why no allowlist for publisher extras.** Allowing publisher-defined extra files would make the layout open-ended and create an evidence-blind channel. Publishers may ship arbitrary content under `data/` (which is read-only, content-hashed, and audited) but must not extend the directory shape.

### §11.9 Manifest re-pointing

**Adversary.** An attacker overwrites `meta.aios.manifest_pointer` to point at a benign-looking manifest while the actual binaries are hostile.

**Mitigation.** §6 manifest pointer discipline. The loader recomputes BLAKE3 over the materialized `manifest.json` and compares to the pointer; mismatch is `MANIFEST_MATERIALIZATION_DRIFT`. Furthermore, the `merkle_root` claimed in `meta.aios` is itself signed by the installer's action envelope (§8.2 `installer_action_id`), so swapping `meta.aios` requires forging the action envelope signature — bound to S0.1 envelope discipline.

## §12 Bounded-cardinality telemetry

Telemetry from this layer is bounded by closed enums; no high-cardinality strings escape into metrics. The following metrics are queued for Prometheus/Loki integration via S3.1 (the L9 evidence log spec carries the binding):

| Metric                                     | Type      | Labels (closed)                                                                                      |
| ------------------------------------------ | --------- | ---------------------------------------------------------------------------------------------------- |
| `aios_package_objects_total`               | gauge     | `kind` (PackageObjectKind), `state` (PackageObjectState), `install_scope`                            |
| `aios_package_load_total`                  | counter   | `kind`, `state`, `result` (success / hash_mismatch / layout_corruption)                              |
| `aios_package_load_latency_seconds`        | histogram | `kind`, `result`                                                                                     |
| `aios_package_state_transitions_total`     | counter   | `from_state`, `to_state`                                                                             |
| `aios_package_promotions_total`            | counter   | `result` (success / verification_failed / approval_denied / conflict)                                |
| `aios_package_rollbacks_total`             | counter   | `rollback_kind`, `result` (success / blocklisted / forbidden / approval_denied)                      |
| `aios_package_quarantines_total`           | counter   | `reason` (capability_lie / hash_mismatch / state_corruption / breach / takedown / layout_corruption) |
| `aios_package_private_state_bytes`         | gauge     | `install_scope` (no per-package label — unbounded cardinality forbidden)                             |
| `aios_package_rollback_peers_total`        | gauge     | `rollback_kind`                                                                                      |
| `aios_package_retention_seconds_remaining` | histogram | `kind`                                                                                               |

Per-package labels (e.g. `package_id`) are forbidden in metric label sets. Per-package facts are recorded in S3.1 evidence records (which are queryable by `package_id`) — not in time series. This keeps metrics cardinality-bounded across deployments with thousands of packages.

## §13 Cross-spec touch-ups (queued)

This contract requires three touch-ups across other contracts. They are listed here for the next refinement wave; until applied, the corresponding S12.2 features are deferred.

### §13.1 S4.1 — User-scope `apps/`

Add `USR_APPS = 9` to `UserReservedName` (S4.1 §6). Add `apps/` to the per-user reserved subdirectories list (S4.1 §6 layout block). Extend §11.10 (uniqueness) to include user-scope: `app_id` is unique within `/aios/groups/<g>/users/<u>/apps/`. Extend §12.10 (L6 constraints) `installable_scope` mapping to include `USER_SCOPED → /aios/groups/<g>/users/<u>/apps/<app_id>/`.

Until applied, S11.1 §5 step 6 rejects `USER_SCOPED` installs with `InstallScopeViolation` carrying sub-reason `UserScopeAppsNotYetEnabled`.

### §13.2 S2.4 — `PACKAGE_OBJECT_LAYOUT_INTACT` property

Add `PACKAGE_OBJECT_LAYOUT_INTACT` to the closed `PropertyType` enum (S2.4). The property holds for a package object iff:

1. The closed file set (§4.2) matches exactly (no missing required, no extras).
2. `meta.aios.manifest_pointer` resolves and `BLAKE3(manifest.json) == manifest_pointer`.
3. The Merkle root over `code/ + data/ + config/ + probes/` matches `meta.aios.merkle_root`.
4. `state.aios` parses; the most recent transition is consistent with `meta.aios.kind`.
5. `rollback.json` parses; each pointer resolves; each pointer's referenced peer exists or is gracefully retired.

The property is verified by the loader at every load (§9) and on-demand by S2.4 schedule.

### §13.3 S3.1 — `PACKAGE_*` record types

Ten record types are queued for the next S3.1 wave (§14).

## §14 Evidence record types (queued for S3.1)

Ten record types are queued for the next S3.1 wave. The retention class is the table-of-truth column; the trigger column lists the operational moment.

| Record type                              | Retention class | Trigger                                                                                                                                                    |
| ---------------------------------------- | --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `PACKAGE_OBJECT_CREATED`                 | `STANDARD_24M`  | First-time installation of a package object completes (active or system; per-scope).                                                                       |
| `PACKAGE_OBJECT_UPDATED`                 | `STANDARD_24M`  | A `STAGED_UPDATE` is promoted to `ACTIVE`. Carries `from_version`, `to_version`, `installer_action_id`.                                                    |
| `PACKAGE_OBJECT_ROLLED_BACK`             | `FOREVER`       | A package object transitions `ACTIVE → ROLLED_BACK` (and its peer `SUPERSEDED → ACTIVE`). Carries reason, target version, blocklist consult result.        |
| `PACKAGE_OBJECT_QUARANTINED`             | `FOREVER`       | A package object transitions to `QUARANTINED`. Carries reason (capability_lie / hash_mismatch / state_corruption / breach / takedown / layout_corruption). |
| `PACKAGE_PRIVATE_STATE_INITIALIZED`      | `STANDARD_24M`  | First-launch initialization of `state/` completes.                                                                                                         |
| `PACKAGE_PRIVATE_STATE_CORRUPT_DETECTED` | `FOREVER`       | Cross-subject write into `state/` detected. Carries `attacker_subject_canonical_id`, `audit_subpaths_violated`.                                            |
| `PACKAGE_VERSION_DOWNGRADE_BLOCKED`      | `FOREVER`       | A rollback target was rejected because the version is on the `RollbackBlocklist`. Carries `cve_reference`, `target_version`.                               |
| `PACKAGE_OBJECT_RETIRED`                 | `EXTENDED_60M`  | A `_rollback_<n>/` peer is retired (post-30d). Also emitted on operator-driven uninstall.                                                                  |
| `PACKAGE_OBJECT_VERIFICATION_FAILED`     | `EXTENDED_60M`  | Per-load Merkle mismatch or staged-peer verification failure. Carries `reason` and recomputed-vs-claimed hashes.                                           |
| `PACKAGE_RECOVERY_RESTORE_PERFORMED`     | `FOREVER`       | A `QUARANTINED` package was restored under recovery mode to a prior version. Carries `recovery_operator_canonical_id`, `target_version`.                   |

These ten record types are added on top of S11.1's nineteen package-distribution record types and S12.1's fourteen ecosystem-runtime record types, bringing the package-related evidence vocabulary to forty-three entries (across S11.1 + S12.1 + S12.2). Wave consolidation is performed by the next S3.1 refinement.

## §15 Worked examples

### §15.1 Example A — Steam game install via `RUNTIME_WINDOWS_PROTON`

**Scenario.** Operator (a member of group `family`) installs Factorio (a Windows-native game) onto an AIOS host, group-scoped. Steam-hosted Windows binary, AIOS community recipe (`recipe_factorio_v3` curated by AIOS) translates the Steam app into an AIOS PackageManifest with `EcosystemRuntime = RUNTIME_WINDOWS_PROTON`.

**Trace.**

1. **L7 marketplace surface** receives operator's install action. Action envelope:

   ```text
   action_id   = act_a1f3...
   subject     = family:alice
   verb        = aios.package.install
   target      = factorio
   parameters  = {
     repository_kind: AIOS_COMMUNITY_REPO,
     manifest_pointer: pkgm_b3c4...,
     ecosystem_runtime: RUNTIME_WINDOWS_PROTON,
     install_scope: GROUP_SCOPED,
     group_id: family,
     update_channel: STABLE,
     rollback_kind: MULTI_VERSION,
   }
   ```

2. **S11.1 install pipeline** runs the seventeen steps. Outcomes: signature verified (`VERIFIED_PUBLISHER`); chain depth 2 (publisher → AIOS root); content hash matches; capability declaration parses (`gamepad`, `audio`, `gpu_compute_low`).
3. **S2.3 policy** routes the install action through approval. The operator (Alice) approves with EXACT_ACTION binding.
4. **Pipeline transitions** `INSTALLING → ACTIVE`. S12.2 engine takes over.
5. **S12.2 layout** — engine creates `/aios/groups/family/apps/factorio/` per §4.2:
   - `meta.aios` (kind=INSTALLED_PACKAGE; bound_subject=`family:apps:factorio`)
   - `manifest.json` (frozen S11.1 manifest)
   - `merkle.aios` (BLAKE3 over `code/ + data/ + config/ + probes/`)
   - `code/factorio.exe`, `code/Factorio.dll`, ...
   - `data/data/`, `data/locale/`, ...
   - `config/config.ini`
   - `state/` (empty; will initialize on first launch)
   - `probes/factorio.probe`
   - `rollback.json` (empty `prior_versions`)
   - `sandbox.json` (composed: Proton runtime floor + game-specific rules)
   - `network.json` (Steam multiplayer + mod portal allowlist)
   - `capabilities.json` (`gamepad`, `audio`, `gpu_compute_low`)
   - `ecosystem.json` (binding to `runtime_windows_proton_v8.0.4` adapter package)
   - `recipe.json` (binding to `recipe_factorio_v3` recipe object)
6. **state.aios** = `INSTALLED` (not yet launched).
7. **First launch** — Alice launches the game.
   - state.aios transitions `INSTALLED → ACTIVE`.
   - `state/` is initialized (game's save directory layout from `data/state_init/` template).
   - `PACKAGE_OBJECT_CREATED` and `PACKAGE_PRIVATE_STATE_INITIALIZED` evidence emitted.
   - The Proton runtime adapter loads the sandbox profile, mounts the Wine prefix at `state/wine_prefix/` (per S4.1 §12.7 — runtime path under the package's scope), launches `factorio.exe`.
8. **Per-load verification** — every subsequent launch recomputes the Merkle (cached at sixty-second TTL); state of game saves under `state/saves/` is preserved across launches; cross-package read of `/aios/groups/family/apps/<other>/state/` is denied by the sandbox.

**Acceptance signal:** evidence query for action `act_a1f3...` returns the `PACKAGE_OBJECT_CREATED` record; package object loader on subsequent launch returns success with cached Merkle hit; sandbox denies a probe read of a peer package's state directory; FOREVER `PACKAGE_PRIVATE_STATE_CROSS_PACKAGE_ACCESS_DENIED` is emitted on the denied peer-read attempt.

### §15.2 Example B — System service install (queued S15.1 unit manifest)

**Scenario.** AIOS root pushes a system-scope service package (`prometheus-aios-exporter`, a metrics exporter for L9 observability). System-scope install requires recovery mode per S4.1 §10 (`SYSTEM_ONLY` installable scope) and INV-012.

**Trace.**

1. **Recovery mode** is entered by the recovery operator (cosigned).
2. **L7 recovery surface** issues the install action:

   ```text
   action_id   = act_d4e5...
   subject     = _system:recovery:bob
   verb        = aios.package.install
   target      = prometheus-aios-exporter
   parameters  = {
     repository_kind: AIOS_ROOT_REPO,
     manifest_pointer: pkgm_e6f7...,
     ecosystem_runtime: RUNTIME_LINUX_NATIVE,
     install_scope: SYSTEM_ONLY,
     update_channel: RECOVERY_CRITICAL,
     rollback_kind: RECOVERY_ONLY,
     service_unit: TRUE,
   }
   ```

3. **S11.1 pipeline** verifies AIOS_ROOT chain. `VERIFIED_AIOS_ROOT`.
4. **S2.3 policy** under recovery mode: cosign approval (recovery operator + AIOS root cosigned takedown ceremony, here used in install direction).
5. **S12.2 layout** — engine creates `/aios/system/apps/prometheus-aios-exporter/` per §4.2 with one additional pointer:
   - `unit.aios` — BLAKE3 address pointing at the future S15.1 unit manifest object. S12.2 only checks the pointer resolves and is non-zero.
   - `attestation.aios` — recovery-install receipt (FOREVER trail).
6. **state.aios** = `INSTALLED`. Auto-launch by the queued S15.1 service supervisor on next normal-mode boot.
7. **On normal-mode boot** — state.aios transitions `INSTALLED → ACTIVE`. The S15.1 supervisor launches the service per its unit manifest (S12.2 is opaque; it only knows the pointer is valid).
8. **Per-load verification** runs on every supervisor restart cycle. The service's `state/` (where it caches metric histories) is bound to `_system:prometheus-aios-exporter`.

**Acceptance signal:** the package object exists; the unit pointer resolves; `attestation.aios` is signed and queryable; `PACKAGE_OBJECT_CREATED` evidence emitted with `install_scope = SYSTEM_ONLY` and `is_recovery_mode = true`. Rollback is `RECOVERY_ONLY`: a future operator request to roll back must enter recovery mode.

### §15.3 Example C — Update with rollback

**Scenario.** The Factorio package from Example A receives an update to version `1.1.107`. The update is staged. The operator promotes it. Two days later, the operator finds the new version unstable and rolls back. State changes since the rollback boundary are lost.

**Trace.**

1. **Day 0, T0**: a `STAGED_UPDATE` action arrives:

   ```text
   action_id    = act_f8g9...
   subject      = family:alice
   verb         = aios.package.update.stage
   target       = factorio
   parameters   = { manifest_pointer: pkgm_h0i1..., new_version: 1.1.107 }
   ```

   S11.1 pipeline validates the new bundle. S12.2 writes `<package_id>/_staged/` per §5.4. state.aios = `STAGED`.

2. **Day 0, T0 + 2 min**: a separate `aios.package.update.promote` action arrives. S2.3 routes through operator EXACT_ACTION approval (STABLE channel). Alice approves.
3. **S12.2 promotion** — atomic three-step rename:
   - `<factorio>/` → `<factorio>/_rollback_1/` (state SUPERSEDED, version 1.1.106)
   - `<factorio>/_staged/` → `<factorio>/` (state ACTIVE, version 1.1.107)
   - `state/` is inherited from `_rollback_1/state/` via S1.3 copy-on-write pointer (manifest declared `state_inheritance = TRUE` for matching major version).
4. **Day 0..2**: operator plays the new version; state changes accumulate (new game saves, new mod data) under `<factorio>/state/`. S1.3 records every save's content addresses.
5. **Day 2, T0**: operator decides to roll back. Action:

   ```text
   action_id    = act_j2k3...
   subject      = family:alice
   verb         = aios.package.rollback
   target       = factorio
   parameters   = { rollback_to: PRIOR }
   ```

   S12.2 validates against `meta.aios.rollback_kind = MULTI_VERSION` (so any peer is admissible; PRIOR is fine). Consults `RollbackBlocklist` for version 1.1.106 → not blocklisted.

6. **Operator preview** — L7 surfaces the state-loss preview: "Rolling back will restore game state as of 2026-05-07 14:30:12. The following changes since then will be lost: 3 new save files (12 MB), 1 mod added (45 MB)." Operator confirms.
7. **S12.2 rollback** — atomic three-step rename:
   - `<factorio>/` → `<factorio>/_rolled_back_1/` (state ROLLED_BACK, version 1.1.107). state/ retained read-only for forensic.
   - `<factorio>/_rollback_1/` → `<factorio>/` (state ACTIVE, version 1.1.106). state/ snapshot from supersede moment is restored.
8. **Evidence emitted**: `PACKAGE_OBJECT_ROLLED_BACK` (FOREVER), recording the rollback target, the blocklist consult result, the state-loss preview Alice acknowledged, the operator id.
9. **Day 32**: retention engine retires `_rolled_back_1/` and `_rollback_2/` (oldest peer). `PACKAGE_OBJECT_RETIRED` (`EXTENDED_60M`) emitted for each retired peer.

**Acceptance signal:** rollback succeeds; the active peer is now version 1.1.106; the operator's pre-rollback state changes are evidenced in the retired `_rolled_back_1/` peer for forensic queries during its retention window; cross-version state preservation works via S1.3 chunk reuse without copying.

## §16 Open questions deferred

These are intentionally out of scope for S12.2 and tracked elsewhere:

- **Service unit manifest schema** — the shape of `unit.aios` content. Queued for S15.1 (a future spec under L6 or L9). S12.2 only pins the pointer location.
- **Cross-host state federation** — when a user moves between AIOS hosts, should `state/` be portable? Deferred; current contract is one-host-one-state.
- **Multi-tenant package sharing** — when two groups want to share a package install (rather than each holding its own copy), the contract is one-install-per-scope. Deferred to a future federation spec.
- **State export/import format** — operator-driven export of `state/` for backup or migration. Deferred; placeholder under §10.7 step 3.
- **Snapshot-driven rollback compression** — the current contract retains full `state/` snapshots per supersede peer (modulo S1.3 chunk reuse). A future optimization could use binary-delta compression. Deferred.
- **Application-level state corruption detection** — the contract detects unauthorized writes (§7.4) but not in-application corruption (e.g. a buggy save format). Deferred to per-application probes.
- **Live-update of running packages** — the current contract requires a re-launch after promotion. Live-update (e.g. for long-running services with no acceptable downtime) is deferred to a future S15.x.
- **Telemetry of private-state size growth** — bounded under `aios_package_private_state_bytes` aggregate but not per-package. Per-package time-series is deferred to a future per-package observability sub-spec.

## See also

- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S1.3 — AIOS-FS Object Model](../L2_AIOS_FS/01_object_model.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S2.4 — Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S3.2 — Sandbox Composition](04_sandbox_composition.md)
- [S4.1 — Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S6.4 — Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md)
- [S8.1 — Network Policy](../L8_Network_Hardware_Devices/02_network_policy.md)
- [S11.1 — Repository Model + Trust Roots](../L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [S12.1 — App Runtime Model](01_app_runtime_model.md)
- [Rev.1 §17 — Application, Package, and Compatibility Model](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [L6 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

Status: REAL
Evidence: E1
