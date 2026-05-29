# Rev.3 — Planning Notes

| Field       | Value                                                         |
| ----------- | ------------------------------------------------------------- |
| Status      | `PLANNING` + holistic Rev.3 spec + materialized Rev.3 contracts for S16-S20 |
| Created     | 2026-05-28                                                    |
| Predecessor | `002.AI-OS.NET--SPECREV.2/` (CONTRACT, M16/M17/M18 in-flight) |
| Trigger     | Operator brainstorming during M16 (T-154) — what to expand    |

This is a **planning notes** document, not a sub-spec. It records the technical scoping
analysis for Rev.3 ahead of Rev.2 completion. Authoritative sub-specs will be written
later, one per topic, under the L0–L10 layer folders mirroring Rev.2 structure.

## Cross-reference — integration framework lands in Rev.2 (M18.5)

Per Governor decision 2026-05-28, the **integration-as-process** framework (typed
`IntegrationLifecycleState` FSM, `VendorIntegrationContract`, `ExternalStandardSubscription`
registry, 3 new evidence types) lands in Rev.2 as a new sub-spec **S11.4** + a new milestone
**M18.5** (`aios-integration`, T-163..T-174) inserted between M18 and Rev.2 FULL-REAL.

Rationale: L10 trust roots + bridges already exist in Rev.2; pushing the integration
process to Rev.3 would force a retroactive change to a frozen L10 boundary. Putting the
contract in Rev.2 and the operationalisation in Rev.3 keeps Rev.2→Rev.3 purely additive.

Rev.3 category 7 (below) **operationalises** what Rev.2 M18.5 frames — live CVE feed
ingestion, SIEM bridges, OpenTelemetry export, STIG/NIST control map maintenance.
See `MILESTONES.md` row M18.5 for the implementation entry.

---

## Brainstorm context

During M16 mid-flight (T-154 ExposureApprovalState FSM dispatched, T-155..T-162 still
pending), the operator asked: "what could Rev.3 develop?"

The principle from the operator is firm: **finish Rev.2 first** (M16 + M17 + M18 closures,
~3 implementation days at current claude-ds tempo). Rev.3 begins only after Rev.2 has
landed at FULL-REAL across all 18 milestones.

This file captures the technical menu derived from that brainstorm. It is the input
for the eventual Wave 1 of Rev.3 spec authoring.

The current holistic Rev.3 architecture is captured in
[`00_REV3_HOLISTIC_SPEC.md`](00_REV3_HOLISTIC_SPEC.md).

---

## The six categories considered for Rev.3 inclusion

### 1. Explicitly deferred items in Rev.2 (low-risk, just finish what was scoped-out)

| Topic                                      | Sub-spec  | What is deferred today                                  |
| ------------------------------------------ | --------- | ------------------------------------------------------- |
| Threshold / multi-party root signing       | L10 S11.1 | "single-host-single-root for now" — explicitly deferred |
| Distributed package mirrors with consensus | L10 S11.1 | deferred                                                |
| Voice + Mobile renderers                   | L7        | mentioned, no contract                                  |
| STRONG approval mechanics (S5.3)           | L4        | referenced, marked deferred                             |
| Per-publisher reputation ledger            | L6 S6.5   | weight ledger partially described                       |

### 2. Horizontal expansion (new form factors)

- **AIOS-on-mobile** — phone / tablet as gold-tier renderer. Wayland-based mobile compositor
  (KDE Plasma Mobile / phosh)
- **AIOS-on-edge** — minimal headless (kiosk, IoT). Pushes Recovery + Cognitive Core to optional
- **AIOS cluster** — multi-host federation. Shared trust roots; distributed evidence logs

### 3. Depth in existing layers

- **Hardware attestation** — TPM 2.0 + SGX/SEV/TDX. Real remote attestation instead of
  firmware-pinned root
- **Federated identity** — SSO across hosts in a cluster / organization (vs. local identity model)
- **Time as constitutional plane** — Trusted NTP / roughtime + clock skew detection. Today
  `Utc::now()` is taken on faith
- **Energy / power policy** — per-app energy budgets; battery-mode capability restrictions
- **Backup / DR plane** — Constitutional backup contract (encrypted, content-addressed, off-host).
  Today only recovery boundary is covered

### 4. Cognitive expansion (L5)

- **Federated model marketplace** — beyond vault-brokered external; signed model bundles,
  evaluation harness, public benchmark contracts
- **Multi-agent coordination contract** — Rev.2 has a single CognitiveCore. Multi-agent
  collaboration (planner + executor + reviewer) as constitutional pattern
- **AI evaluation evidence** — record types for model accuracy drift, hallucination rates,
  prompt-injection rejection

### 5. Compliance / legal

- **GDPR contract** — explicit EU data residency, RTBF (right to be forgotten) procedure
- **Audit log export schema** — compliance with SOC2 / ISO 27001 / HIPAA reporting requirements
- **Cross-org trust delegation** — corporate fleet deployment

### 6. Additional ecosystem runtimes

- `RUNTIME_WASM_NATIVE` — WebAssembly host (Wasmtime / wasmer); safer than Linux native
- `RUNTIME_EBPF_NATIVE` — eBPF-native apps (kernel-side, deeply sandboxed)
- `RUNTIME_DENO` / `RUNTIME_BUN` — JS sandbox with capability mapping
- `RUNTIME_PYTHON_NATIVE` — Python sandbox via PEP 711 / pyodide

### Kernel portability extension

Operator direction on 2026-05-28: AIOS should investigate whether the system can
be agnostic toward the kernel, including BSD and RTOS-style targets.

Scoping decision:

- Linux remains the primary implementation target for desktop, gaming, GPU,
  video, containers, and broad hardware support.
- Kernel-specific behavior must move behind signed backend adapters.
- BSD, RTOS, microVM, userspace-kernel, WASI, unikernel, and high-assurance
  microkernel targets are admitted through a `KernelCapabilityMatrix`, not by
  pretending all kernels expose the same primitives.
- The universal mechanism is materialized as
  [`S18 Kernel Personality and Portability Plane`](S18_Kernel_Personality_Portability/00_overview.md).

---

## Security hardening audit — SELinux + STIG-grade baseline

Operator question on 2026-05-28: **can AIOS use SELinux, and can AIOS use a
military-grade standard to make the system secure?**

Short answer:

- **Yes, SELinux should be the default primary MAC layer for the high-security
  Rev.3 profile.** Rev.2 already names SELinux in `LsmConfig` and sandbox backend
  capability probing, but only as a boolean. Rev.3 should promote it into a real
  policy plane with labels, domains, policy module lifecycle, evidence, and
  recovery-gated policy mutation.
- **Yes, AIOS can align to military / government hardening baselines, but must
  not claim certification by wording alone.** The correct target is
  **DISA STIG-aligned** plus a NIST control map. Formal DoD ATO, Common Criteria,
  or FIPS validation are separate certification processes.

External baselines checked for Rev.3 scoping:

| Baseline                                                                                                         | Rev.3 use                                                                                                              |
| ---------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| [SELinux upstream](https://github.com/SELinuxProject/selinux)                                                    | Primary Linux mandatory access control backend for high-security profile.                                              |
| [DISA STIGs](https://public.cyber.mil/stigs/)                                                                    | "Military-grade" configuration baseline; use as hardening target and evidence checklist, not as a certification claim. |
| [NIST SP 800-53 Rev. 5](https://csrc.nist.gov/pubs/sp/800/53/r5/upd1/final)                                      | Control catalog map for policy, audit, access control, system integrity, incident response, and supply-chain controls. |
| [NIST SP 800-218 SSDF](https://csrc.nist.gov/pubs/sp/800/218/final)                                              | Secure development and release process for AIOS packages, adapters, kernels, and policy bundles.                       |
| [NIST SP 800-207 Zero Trust](https://csrc.nist.gov/pubs/sp/800/207/final)                                        | Cluster/fleet access model: no implicit LAN trust, per-request authorization, continuous posture checks.               |
| [NIST SP 800-193 firmware resiliency](https://csrc.nist.gov/pubs/sp/800/193/final)                               | Firmware protection/detection/recovery model for L8.5 + L1 boot trust.                                                 |
| [FIPS 140-3 / CMVP](https://csrc.nist.gov/projects/cryptographic-module-validation-program/fips-140-3-standards) | Optional strict crypto profile using validated modules; separate from the default open-source crypto profile.          |
| [CIS Controls v8](https://www.cisecurity.org/controls/v8)                                                        | Practical non-government baseline for community deployments and operator dashboards.                                   |

### Audit findings against current Rev.2/Rev.3 plan

| Finding                                                            | Current state                                                                                                                                                               | Rev.3 improvement                                                                                                                                                                                                                                                                                |
| ------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `SEC-001` SELinux is named but not modeled                         | S9.3 has `selinux_enforcing: bool`; S3.2 probes SELinux as `ABSENT/PERMISSIVE/ENFORCING`. There is no AIOS SELinux type system.                                             | Add `AIOS_MAC_POLICY` sub-spec: SELinux domains, labels, MCS/MLS categories, policy module signing, AVC-to-evidence translation, and recovery-only policy changes.                                                                                                                               |
| `SEC-002` No hardening profile matrix                              | Rev.2 has many constitutional invariants but no distro-wide profile such as dev/default/STIG/airgap.                                                                        | Add `SecurityProfile` enum: `DEV_RELAXED`, `SECURE_DEFAULT`, `STIG_ALIGNED`, `AIRGAP_HIGH`. Every profile fixes LSM, crypto, network, update, audit, and boot posture.                                                                                                                           |
| `SEC-003` STIG/NIST controls are not mapped                        | Rev.2 evidence and policy are strong, but auditors cannot see which NIST/STIG controls each mechanism satisfies.                                                            | Add a control map: AIOS invariant / policy rule / evidence record / verification primitive / NIST 800-53 family / STIG check reference.                                                                                                                                                          |
| `SEC-004` Boot integrity is split across specs                     | S9.3 covers kernel drift; S8.5 covers firmware trust; TPM attestation is deferred.                                                                                          | Combine Secure Boot, TPM PCRs, kernel lockdown, signed modules, IMA/EVM appraisal, dm-verity/IPE where available, and remote attestation evidence into one hardening contract.                                                                                                                   |
| `SEC-005` FIPS boundary is not defined                             | Code and specs use Ed25519, BLAKE3, AES-GCM, HMAC-SHA256, HKDF-SHA256, and X25519 through normal Rust crates. That is not the same as a FIPS 140-3 validated crypto module. | Add optional `FIPS_STRICT` profile. It uses a CMVP-validated crypto provider for compliance-sensitive operations and records algorithm/module certificate ids in evidence. Keep BLAKE3 as a content-addressing hash only where the FIPS profile permits, or add parallel SHA-256/SHA-384 fields. |
| `SEC-006` Supply-chain metadata is incomplete for government audit | L10 has signatures, trust roots, capability-lie audit, and deplatforming; SBOM/provenance/VEX are not first-class.                                                          | Require SPDX or CycloneDX SBOM, SLSA-style provenance, dependency vulnerability status, reproducible build receipt, and signed VEX for every package/kernel/adapter release.                                                                                                                     |
| `SEC-007` Service hardening is not scored                          | Sandbox composition exists for actions/apps, but long-running AIOS systemd services do not have a measurable hardening score.                                               | Add systemd unit hardening requirements and `systemd-analyze security` thresholds per service class; failures block promotion in `STIG_ALIGNED`.                                                                                                                                                 |
| `SEC-008` No SCAP/checklist export                                 | Evidence is AIOS-native, but external auditors expect STIG/CIS/NIST-oriented artifacts.                                                                                     | Add `aios-hardening-audit` output formats: JSON evidence, Markdown report, OpenSCAP/XCCDF-style checklist where practical, and STIG Viewer `.cklb` export if the schema remains stable.                                                                                                          |

### SELinux contract shape for Rev.3

SELinux should not replace the AIOS Policy Kernel. It should enforce the kernel
boundary below it.

Proposed domains:

| Domain            | Purpose                                                                                                         |
| ----------------- | --------------------------------------------------------------------------------------------------------------- |
| `aios_policy_t`   | Policy Kernel process; reads signed policy bundles; cannot write evidence directly except through brokered API. |
| `aios_vault_t`    | Vault Broker; owns secret material; only exposes use-without-reveal operations.                                 |
| `aios_evidence_t` | Evidence Log writer; append-only storage; write path inaccessible to AI/app domains.                            |
| `aios_sandbox_t`  | Sandbox Composer/enforcer; sole domain allowed to apply seccomp/namespaces/cgroups/Landlock/SELinux labels.     |
| `aios_runtime_t`  | Capability Runtime and adapters; executes approved typed actions only.                                          |
| `aios_renderer_t` | KDE/Web/CLI renderers; UI only, no direct policy/evidence mutation.                                             |
| `aios_agent_t`    | AI agent processes; strictest network, filesystem, ptrace, device, and secret boundaries.                       |
| `aios_recovery_t` | Recovery shell/services; only active in recovery or first-boot profile.                                         |
| `aios_package_t`  | Package installer; writes only to declared install scopes after policy approval.                                |

MCS/MLS proposal:

- Map `group_id` to SELinux MCS categories so cross-group access is denied by the
  kernel even if a userspace check fails.
- Map high privacy classes (`SECRET_BEARING`, future `CLASSIFIED`) to MLS-like
  levels in the `STIG_ALIGNED` and `AIRGAP_HIGH` profiles.
- Treat every AVC denial involving `aios_agent_t`, `aios_vault_t`, `aios_policy_t`,
  `aios_evidence_t`, or `aios_recovery_t` as evidence-worthy.

Policy lifecycle:

1. SELinux policy modules are AIOS packages of kind `MAC_POLICY_BUNDLE`.
2. Install/update requires recovery mode for system domains.
3. Every module has content hash, signature chain, profile compatibility, and
   rollback metadata.
4. Production builds forbid `audit2allow`-generated policy from being installed
   unless manually reviewed and signed.
5. A policy reload emits `MAC_POLICY_LOADED` evidence with enforcing/permissive
   state, policy hash, module list hash, and active security profile.

### STIG / NIST control map for Rev.3

Rev.3 should define one machine-readable control matrix:

```text
control_id:
  source: NIST_800_53 | DISA_STIG | CIS_V8 | AIOS_NATIVE
  family: AC | AU | CM | IA | SC | SI | SR | ...
  statement: short normative requirement
  enforced_by:
    - policy_rule_id
    - selinux_domain_or_type
    - sandbox_profile_floor
    - kernel_config_option
    - verification_primitive
  evidence:
    - record_type
    - retention_class
  profiles:
    - SECURE_DEFAULT
    - STIG_ALIGNED
    - AIRGAP_HIGH
  status: REAL | PARTIAL | CONTRACT | DEFERRED
```

Initial control families to map first:

| Family                                  | AIOS owner                                                        |
| --------------------------------------- | ----------------------------------------------------------------- |
| Access control (`AC`)                   | L4 Policy Kernel + SELinux MAC + L2 namespace layout              |
| Audit/accountability (`AU`)             | L9 Evidence Log + Rev.3 checklist exporter                        |
| Configuration management (`CM`)         | L10 packages + signed policy/MAC/kernel bundles                   |
| Identification/authentication (`IA`)    | L4 Identity + hardware-key recovery credentials                   |
| System/communications protection (`SC`) | L8 Network Policy + TLS/mTLS/WireGuard                            |
| System/information integrity (`SI`)     | L1 Secure Boot/IMA/EVM + L8 Firmware Trust + package verification |
| Supply-chain risk (`SR`)                | L10 trust roots + SBOM/provenance/VEX                             |

### High-assurance boot and runtime baseline

Rev.3 high-security profile should require:

1. UEFI Secure Boot enabled.
2. Kernel lockdown at `confidentiality` level when Secure Boot is active.
3. Signed kernel modules only; out-of-tree modules recovery-gated and FOREVER-evidenced.
4. TPM 2.0 measured boot with signed quote support.
5. IMA measurement for audit, IMA appraisal/EVM for critical paths, and IPE/dm-verity
   or equivalent immutable-root enforcement where available.
6. `selinux=1 enforcing=1` for the `STIG_ALIGNED` and `AIRGAP_HIGH` profiles.
7. `unconfined_t` forbidden for AIOS-owned services.
8. Root filesystem integrity evidence at every boot.
9. Remote attestation verifier for cluster/fleet mode.
10. Boot failure drops to recovery with evidence, not silent permissive mode.

### Recommended Rev.3 security sub-specs

| Proposed sub-spec                         | Layer                  | Priority | Reason                                                                     |
| ----------------------------------------- | ---------------------- | -------- | -------------------------------------------------------------------------- |
| `S16.1 Security Profile Matrix`           | L0/L1 cross-cutting    | P0       | Defines `DEV_RELAXED`, `SECURE_DEFAULT`, `STIG_ALIGNED`, `AIRGAP_HIGH`.    |
| `S16.2 SELinux MAC Policy Plane`          | L1/L6/L4 cross-cutting | P0       | Turns SELinux from boolean into enforceable distro policy.                 |
| `S16.3 STIG/NIST Control Map + Scanner`   | L9/L10 cross-cutting   | P0       | Gives operators and auditors proof, not claims.                            |
| `S16.4 Measured Boot + Runtime Integrity` | L1/L8                  | P0       | Binds Secure Boot, TPM, IMA/EVM, lockdown, module signing, firmware drift. |
| `S16.5 FIPS/Crypto Boundary Profile`      | L4/L9/L10              | P1       | Needed before any government-grade compliance claim.                       |
| `S16.6 SBOM + Provenance + VEX`           | L10                    | P1       | Makes package trust auditable beyond signatures.                           |
| `S16.7 Service Hardening Score Gates`     | L3/L9                  | P1       | Blocks weak systemd service units from promotion.                          |
| `S16.8 Zero-Trust Fleet Posture`          | L8/L4/L9               | P2       | Required for cluster/cross-org trust, not for single-host MVP.             |

Materialized Rev.3 contracts created on 2026-05-28:

- [`S16.1 Security Profile Matrix`](S16_Security_Hardening_Compliance/01_security_profile_matrix.md)
- [`S16.2 SELinux MAC Policy Plane`](S16_Security_Hardening_Compliance/02_selinux_mac_policy_plane.md)
- [`S16.3 STIG/NIST Control Map + Scanner`](S16_Security_Hardening_Compliance/03_stig_nist_control_map_scanner.md)

### What not to claim yet

- Do **not** claim "DoD certified", "STIG compliant", "FIPS validated", or
  "military certified" until an actual assessment/certification path exists.
- Claiming **"STIG-aligned hardening profile"** is acceptable once the profile,
  scanner, evidence records, and exceptions register exist.
- FIPS 140-3 is about validated cryptographic modules, not just using strong
  algorithms. A self-built Rust crypto stack is not automatically FIPS validated.

---

## Package-agnostic distribution + mobile expansion ideas

Operator direction on 2026-05-28:

> Make the distribution package-agnostic: know all major repositories, accept any
> Linux package ever invented where possible, convert or install directly when
> that is smarter, keep the already-planned foreign OS runtimes, and develop the
> mobile phone interface.

The product principle:

> AIOS should ingest software from any ecosystem, explain the risk, choose the
> safest runtime, and prove what happened. It should not blindly trust every repo
> or run foreign maintainer scripts as root.

### Universal package intake

Rev.3 should add a single intake plane for all known package ecosystems:

| Source format / ecosystem | Default AIOS handling |
| ------------------------- | --------------------- |
| Debian / Ubuntu `.deb` | Parse metadata, maintainer scripts, systemd units, dependencies; convert to AIOS App Object or isolated native install. |
| Fedora / RHEL / openSUSE `.rpm` | Parse spec metadata, scriptlets, systemd units, SELinux labels; convert or install through native compatibility layer. |
| Arch / AUR / `PKGBUILD` | Treat build/install scripts as untrusted; run in build sandbox; convert script intent into typed actions. |
| Alpine `.apk` | Lightweight container/native path; good for edge/headless profiles. |
| Nix / flakes | Prefer reproducible isolated environment; record derivation/provenance in package passport. |
| Flatpak | Import manifest and portals; map permissions to AIOS capabilities. |
| Snap | Import plugs/slots; map to AIOS capabilities and sandbox profile. |
| AppImage | Extract, inspect, sandbox; never broad-host execute by default. |
| OCI / Docker images | Run as app/container object with network/filesystem/device policy; Docker socket never exposed directly. |
| Source releases | Build in hermetic builder; generate SBOM/provenance; output AIOS package. |
| GitHub/GitLab releases | Fetch signed asset when available; otherwise community/untrusted path with high-friction approval. |

Core rule: **catalog everything, trust selectively, execute only through AIOS
policy/sandbox/evidence.**

### Package Rosetta / converter

Add a `PackageRosetta` compiler that converts foreign package semantics into AIOS
objects:

```text
foreign package
  -> metadata parser
  -> script observer
  -> dependency graph resolver
  -> capability extractor
  -> sandbox profile proposal
  -> install/verify/rollback plan
  -> human approval
  -> AIOS App Object
```

Inputs it must understand:

- package metadata (`control`, RPM spec, `PKGBUILD`, `snapcraft.yaml`, Flatpak
  manifest, AppImage desktop metadata, Nix flake, OCI config)
- maintainer scripts (`preinst`, `postinst`, RPM scriptlets, AUR build functions)
- systemd units, timers, sockets, udev rules, dbus services, desktop files
- shared-library dependencies and ABI assumptions
- requested network endpoints
- filesystem write paths
- kernel module / DKMS / firmware needs
- SELinux/AppArmor profile hints where present

Outputs:

- AIOS package manifest
- capability list
- network outbound manifest
- sandbox profile
- rollback plan
- compatibility rating seed
- evidence requirements

### Smart install decision engine

For every candidate package, AIOS should choose the safest viable path:

| Decision | When to use |
| -------- | ----------- |
| `NATIVE_CONVERTED` | Clean Linux app, no system mutation beyond declared paths. |
| `NATIVE_ISOLATED` | Needs native ABI but can run under AIOS sandbox. |
| `FLATPAK_STYLE` | GUI app with portal-compatible permissions. |
| `NIX_ENV` | Reproducible CLI/dev tool or dependency-heavy app. |
| `DISTROBOX_CONTAINER` | App strongly assumes a specific distro userspace. |
| `APPIMAGE_EXTRACTED` | Portable binary with unknown manifest; extract and sandbox. |
| `OCI_CONTAINER` | Server/service packaged as container image. |
| `VM_FALLBACK` | Needs privileged services, fragile ABI, kernel driver, anti-cheat, hard DRM, or unsupported OS behavior. |
| `BLOCKED_WITH_REASON` | Unsafe, unverifiable, legally impossible, or requires host mutation outside policy. |

The operator sees the decision plus alternatives:

```text
Install options for <app>:
  1. Flatpak path      risk low      compatibility high
  2. Debian native    risk medium   compatibility high
  3. AppImage         risk medium   compatibility unknown
  4. Source build     risk low      build time high
```

### Universal App Lab

Unknown software should enter an observation lab before install:

1. Run in maximum-restriction sandbox.
2. Capture attempted filesystem reads/writes, DNS, outbound sockets, GPU init,
   dbus, portals, device opens, process launches.
3. Redact payloads; record only structural behavior.
4. Produce an install recipe proposal.
5. Quarantine on breakout attempts.

This extends S12.1 Phase A from app compatibility into a general package-intake
mechanism.

### Shadow install

Every risky package gets a dry install first:

```text
foreign package install
  -> overlay root / ephemeral namespace
  -> observe scripts and filesystem mutations
  -> translate mutations to typed AIOS actions
  -> verify no forbidden writes
  -> promote or discard
```

Benefits:

- maintainer scripts do not touch the real host
- full rollback is cheap
- dependency graph is known before promotion
- package can be rejected with exact reason

### Install risk diff

Before approval, render a structured diff:

```text
This install requests:
  + network: api.vendor.example:443
  + filesystem write: /aios/groups/work/apps/<app>/
  + systemd service: vendor-sync.service
  + dbus service: org.vendor.Sync
  + GPU class: GPU_FULL_3D
  + background autostart

This install is denied unless operator approves:
  ! broad home read
  ! kernel module build
  ! postinstall script wants /usr/bin mutation
```

The diff is produced from typed facts, not from natural-language guesswork.

### AIOS Package Passport

Every installed app/package should have a passport:

| Field | Meaning |
| ----- | ------- |
| Origin | Repo/source URL, package format, publisher, signature chain. |
| Trust | AIOS trust level, repo reputation, maintainer reputation. |
| Runtime | Native, Flatpak, Nix, Distrobox, Proton, Waydroid, VM, etc. |
| Capabilities | Files, network, GPU, devices, secrets, dbus, portals. |
| Risk | Install risk, runtime risk, supply-chain risk, sandbox drift. |
| Evidence | Last install receipt, last launch receipt, last policy decision. |
| Rollback | Previous version, dependency snapshot, config snapshot. |
| Compatibility | Rating, known issues, recommended runtime, blocked reasons. |

This becomes the operator-facing truth surface for software.

### One app, many backends

AIOS should compare all available versions of the same app:

- distro native package
- Flatpak
- Snap
- AppImage
- Nix package
- source build
- OCI image
- Windows version through Proton
- Android version through Waydroid/VM

The system chooses a recommended path by scoring:

```text
security score
+ compatibility score
+ update reliability
+ rollback quality
+ runtime cost
+ hardware fit
+ user preference
```

This is stronger than "install the first package found".

### AIOS Application Model / how apps are treated

Core decision:

> AIOS should not treat an application as "whatever the package manager
> installed". AIOS should treat every app as a governed workload with identity,
> source, runtime, capabilities, data contract, update policy, rollback, and
> evidence.

Package formats are inputs. The internal truth object is `AIOSAppObject`.

```text
.deb / .rpm / Flatpak / Snap / AppImage / Nix / OCI / Helm / source / PWA /
Windows app / Android app / VM image / game / plugin / driver
  -> intake
  -> AIOSAppObject
  -> runtime plan
  -> policy approval
  -> launch/update/remove with evidence
```

External metadata AIOS should ingest where available:

| Source | Why it matters |
| ------ | -------------- |
| [AppStream metadata](https://www.freedesktop.org/software/appstream/docs/chap-AppStream-About.html) | Cross-distro software metadata for app stores, components, screenshots, categories, releases, firmware, drivers, runtimes. |
| [Desktop Entry specification](https://specifications.freedesktop.org/desktop-entry-spec/latest-single/) | Launchers, names, icons, categories, MIME/file handlers. |
| [XDG Base Directory specification](https://specifications.freedesktop.org/basedir/) | Standard app data/config/cache/state/runtime directory model. |
| [XDG Desktop Portal API](https://flatpak.github.io/xdg-desktop-portal/docs/api-reference.html) | Controlled file, screen, portal, desktop integration for sandboxed apps. |
| Package/container metadata | Dependencies, maintainer scripts, capabilities, services, signatures, SBOM/provenance. |

#### App classes

| App class | Default treatment |
| --------- | ----------------- |
| `SYSTEM_COMPONENT` | Signed AIOS/base/recovery component; immutable or image-managed; user apps cannot depend on mutating it. |
| `TRUSTED_NATIVE_APP` | Native Linux app with declared paths/capabilities and rollback. |
| `PORTAL_GUI_APP` | Flatpak-style GUI app; host access through portals by default. |
| `CLI_TOOL` | Installed into workspace/dev profile; no broad home or secret access unless declared. |
| `BACKGROUND_SERVICE` | Managed as user/system service with explicit network, storage, restart, and log policy. |
| `CONTAINER_APP` | Podman/containerd/OCI workload with `CloudNativePassport`. |
| `K8S_APP` | Kubernetes namespace-scoped workload; admission policy, image trust, secrets, network policy, backup. |
| `GAME_APP` | Game Passport, isolated save/mod/shader/runtime state, no work/family secrets. |
| `WEB_PWA` | Site-as-app with isolated browser profile, storage, permission, and network policy. |
| `ANDROID_APP` | Waydroid/VM/container path; no direct host files without portal/export. |
| `WINDOWS_APP` | Wine/Proton prefix first; VM fallback for hard DRM, anti-cheat, fragile kernel behavior. |
| `VM_APP` | VM image treated as app; integration points are explicit: clipboard, files, USB, GPU, network. |
| `RT_APP` | Requires `RTWorkloadManifest`, admission check, latency evidence, and human approval. |
| `PLUGIN_OR_EXTENSION` | Treated as its own supply-chain object; cannot inherit broad parent trust silently. |
| `AI_AGENT_APP` | Agent actions go through policy/action approval; app identity and agent identity remain separate. |
| `DRIVER_OR_FIRMWARE` | High-risk driver safety plane; signed, measured, rollbackable, recovery-approved. |

#### AIOSAppObject contract

```text
AIOSAppObject
  app_id: stable reverse-DNS or AIOS-generated id
  display: name, icon, category, localized descriptions
  class: system | gui | cli | service | game | container | k8s | vm | web | android | driver | plugin | agent | rt
  source: repo, package format, publisher, signature, URL, mirror
  variants: native, flatpak, nix, appimage, oci, proton, android, vm, source
  selected_runtime: native | portal | nix | podman | containerd | k8s | wine | proton | android | vm | wasm | rt
  trust: repo trust, publisher trust, signature status, SBOM/provenance, vulnerability state
  capabilities: files, network, dbus, portals, devices, GPU, audio, video, secrets, background, admin
  data_contract: config, data, cache, state, logs, export/import, backup, wipe
  workspace_scope: work | gaming | lab | family | admin | airgap | custom
  lifecycle: discovered | staged | approved | installed | running | paused | quarantined | removed
  update_policy: pinned | manual | safe-auto | fleet-approved | blocked
  rollback: previous version, dependency snapshot, config snapshot, data migration plan
  evidence: install receipt, launch receipt, policy decisions, denials, network/device events
```

#### Data treatment

AIOS should separate app code, app config, app state, user documents, secrets,
logs, and cache. The user should never have to guess what deleting an app will
delete.

Recommended model:

| Data type | Treatment |
| --------- | --------- |
| App code | Immutable install scope or runtime image; never mixed with user data. |
| Config | Per-app/per-workspace config, exportable and rollbackable. |
| State | Runtime state and app databases; backup policy shown clearly. |
| Cache | Disposable; never treated as important data. |
| Logs | App/private logs with retention and privacy policy. |
| User documents | Access only through declared paths or document portal. |
| Secrets | Vault/secret broker, never broad environment-variable spray by default. |
| Save games | Separate game data class; sync/backup/mod compatibility policy. |
| VM/container volumes | Named, visible, backup/rollback policy attached. |

Use the XDG model for compatibility, but map it into AIOS workspace-aware
storage so that `Work`, `Gaming`, `Lab`, `Family`, and `Admin` worlds do not
silently share data.

#### Capability treatment

Apps request capabilities, not raw host authority:

| Capability | Default |
| ---------- | ------- |
| Home/files | Denied except app-private scope; document portal or declared folders. |
| Network | Per-profile default; unknown apps get explicit egress manifest. |
| D-Bus/IPC | Denied except declared names/portals. |
| Camera/microphone/screen | Runtime prompt, visible indicator, evidence receipt. |
| GPU/video encode | Declared device class; higher-risk on secure/admin profiles. |
| USB/Bluetooth/serial | Explicit device intent and human approval. |
| Secrets | Brokered secret access; scoped, audited, revocable. |
| Background autostart | Approval required; visible in app passport. |
| System service | Approval required; systemd hardening score recorded. |
| Kernel/firmware | Driver safety plane, recovery approval, boot evidence. |
| RT scheduling | RT admission controller, CPU/IRQ/memory plan, latency evidence. |

#### App lifecycle state machine

```text
DISCOVERED
  -> OBSERVED_IN_LAB
  -> STAGED
  -> APPROVED
  -> INSTALLED
  -> RUNNING
  -> UPDATED
  -> QUARANTINED or ROLLED_BACK or REMOVED
```

Lifecycle rules:

- Discovery never implies trust.
- Staging never mutates the real host.
- Approval signs the exact install/update/runtime plan.
- Launch emits a runtime receipt.
- Quarantine preserves evidence and user data unless the operator chooses wipe.
- Remove must offer: remove app only, remove app + config, remove all app data.

#### Update treatment

Apps should not all update the same way.

| App risk | Update policy |
| -------- | ------------- |
| Low-risk GUI app | Safe auto-update if signed, rollbackable, no new capabilities. |
| Medium-risk app | Manual approval if capabilities, services, or data migration change. |
| Admin/security app | Fleet/profile approval and evidence retention. |
| Driver/firmware/kernel-adjacent app | Recovery-approved, boot-tested, rollback gate. |
| Game/runtime stack | Per-game compatibility snapshot before update. |
| Container/K8s app | Digest-pinned; rollout/rollback policy required. |
| Lab/untrusted app | Disposable update or rebuild from source with evidence. |

If an update requests new capabilities, AIOS treats it like a new install risk
diff, not like a routine patch.

#### App Store treatment

The App Lab UI should show one app with many possible backends, not many random
entries:

```text
Blender
  recommended: Flatpak/portal path
  alternatives: distro package, AppImage, source build, container
  trust: verified upstream
  capabilities: GPU, files via portal, optional camera/screen for capture
  rollback: available
  known issues: none on this GPU
```

Matching signals:

- AppStream id / component id
- desktop file id
- upstream URL
- publisher signature
- binary/package name aliases
- icon/name/category similarity
- repository metadata
- community compatibility evidence

#### Non-negotiable app rules

- No app gets direct root by being "installed".
- No app gets broad home access by default.
- No app gets the Docker socket by default.
- No app sees work/family/admin secrets from gaming or lab workspaces.
- No plugin silently inherits full parent trust.
- No post-install script mutates host paths without typed translation.
- No service autostarts without being visible in the passport.
- No package is considered installed until rollback is known.
- No UI renderer can grant a capability directly; it can only request a signed
  policy decision.

Product name:

> **AIOS App Control Plane** — one model for packages, apps, services, games,
> containers, Kubernetes workloads, VMs, web apps, plugins, agents, and drivers.

### App Capsule / mini-container runtime model

Operator idea on 2026-05-28:

> Treat applications like mini-containers. Especially for Windows applications,
> put everything needed for the application to run inside that unit.

This is the right abstraction, but it should not mean "Docker container for
every desktop app". GUI apps, games, audio/video, GPU acceleration, file
pickers, controllers, fonts, secrets, and screen sharing need a richer capsule
than a server container.

AIOS term:

> **App Capsule** — a per-application runtime envelope containing code,
> dependency layers, config/state boundaries, sandbox policy, device/network
> grants, update plan, rollback plan, and evidence.

Reference patterns:

| Existing pattern | Lesson for AIOS |
| ---------------- | --------------- |
| [Flatpak sandboxing](https://docs.flatpak.org/) | Apps can be isolated and still use host services through portals. |
| [Wine prefixes](https://wiki.winehq.org/FAQ) | Windows apps already work best with per-app/per-family prefixes. |
| [Valve Proton](https://github.com/ValveSoftware/Proton) | Windows games need a curated Wine-based compatibility stack, not raw Wine only. |
| [Bottles environments](https://docs.usebottles.com/getting-started/environments) | Windows apps benefit from named environments such as Gaming/Application plus preinstalled dependencies. |
| [Bottles dependencies](https://docs.usebottles.com/bottles/dependencies) | Compatibility improves when vcredist, dotnet, d3dx, fonts, media components, and runners are managed as explicit dependencies. |

#### Capsule types

| Capsule type | Use case | Runtime contents |
| ------------ | -------- | ---------------- |
| `LINUX_NATIVE_CAPSULE` | Normal Linux GUI/CLI app. | App code, declared libs, XDG data/config/cache/state, portals, SELinux/seccomp/cgroup policy. |
| `FLATPAK_STYLE_CAPSULE` | GUI app with portal-friendly behavior. | Runtime/base layer, app layer, portal permissions, private XDG dirs. |
| `NIX_CAPSULE` | Dev tools and reproducible dependency-heavy apps. | Nix closure, profile binding, declared workspace paths. |
| `APPIMAGE_CAPSULE` | Portable binary with weak metadata. | Extracted AppImage, generated manifest, restricted sandbox. |
| `OCI_CAPSULE` | Server/service style app. | OCI image, rootless Podman/containerd, volumes, network policy. |
| `WINDOWS_APP_CAPSULE` | Windows productivity/legacy app. | Wine runner, per-app prefix, registry, DLLs, fonts, vcredist/dotnet/media deps, file bridge. |
| `WINDOWS_GAME_CAPSULE` | Windows game. | Proton/Wine-GE runner, prefix, DXVK/VKD3D, shader cache, controller mapping, save data, anti-cheat status. |
| `ANDROID_CAPSULE` | Android app. | Waydroid/VM/container user data, permission bridge, file/export policy. |
| `VM_CAPSULE` | Unsafe/fragile/other OS app. | VM image, snapshots, clipboard/file/USB/GPU integration policy. |
| `PLUGIN_CAPSULE` | Browser/editor/IDE/app plugin. | Plugin package, parent binding, separate trust/capabilities, rollback. |
| `RT_CAPSULE` | Real-time workload. | RT manifest, CPU/IRQ/memory/device reservation, latency evidence. |

#### Capsule manifest

```text
AppCapsule
  capsule_id
  app_id
  capsule_type
  source_artifacts
  base_runtime
  dependency_layers
  selected_runner
  filesystem_view
  xdg_mapping
  registry_or_config_state
  capabilities
  device_policy
  network_policy
  secrets_policy
  update_policy
  rollback_snapshots
  evidence_links
```

#### Windows capsule contents

A Windows app capsule should bundle or reference:

| Component | Treatment |
| --------- | --------- |
| Runner | Wine, Proton, Proton-GE/Wine-GE-like runner, or vendor-tested runner. |
| Prefix | Per-app or per-app-family Wine prefix; never one global `~/.wine`. |
| Registry | Versioned registry snapshot with rollback. |
| DLL overrides | Declared and evidenced; no invisible host mutation. |
| Dependencies | vcredist, dotnet, msxml, d3dx, d3dcompiler, fonts, media components where legally distributable. |
| Graphics | DXVK/VKD3D/OpenGL path, GPU class grant, shader cache scope. |
| Audio/video | PipeWire bridge, codec availability, capture/stream permissions. |
| Files | Controlled document/export bridge, not full home access by default. |
| Network | Endpoint or platform manifest where possible. |
| Saves/state | Separate backup/sync/rollback policy. |
| Anti-cheat/DRM | Compatibility truth field: supported, unknown, blocked, VM-hostile, vendor-refuses. |

Default rule:

```text
one Windows app/game
  -> one capsule
  -> one prefix
  -> one dependency recipe
  -> one rollback chain
  -> one compatibility record
```

Shared prefixes are allowed only for an app suite that explicitly needs shared
state, for example an old office suite or launcher/game pair. Shared prefix
membership must be visible in the passport.

#### Thin vs fat capsules

AIOS should support two packaging modes:

| Mode | Meaning | Use case |
| ---- | ------- | -------- |
| `THIN_CAPSULE` | Capsule references signed shared runtime layers. | Normal apps, saves disk, easier updates. |
| `FAT_CAPSULE` | Capsule carries all required runtime/dependency layers. | Airgap, fragile Windows apps, old enterprise apps, reproducible forensic installs. |

Windows legacy software should default to `FAT_CAPSULE` when dependency drift is
the main risk. Modern Linux apps should default to `THIN_CAPSULE` where shared
runtimes are safe.

#### Capsule filesystem layout

```text
/aios/apps/<app_id>/
  capsule.toml
  code/                 immutable or image-backed
  runtime/              selected runtime metadata
  deps/                 dependency layer references
  state/                app state
  config/               app config
  cache/                disposable cache
  logs/                 private logs
  exports/              user-approved exported files
  snapshots/            rollback points
```

For Windows capsules:

```text
/aios/apps/<app_id>/windows/
  prefix/
  registry/
  drive_c/
  dll-overrides.toml
  runner.toml
  win-deps.lock
  shader-cache/
  save-data/
```

#### Capsule lifecycle

```text
discover app
  -> build capsule recipe
  -> dry-run in App Lab
  -> solve dependencies
  -> create capsule
  -> run first-launch probe
  -> record compatibility
  -> promote to workspace
  -> update/rollback/quarantine as a unit
```

#### Capsule security rules

- Capsule isolation is the default for every non-system app.
- Capsule does not imply trust; it only limits blast radius.
- No capsule gets broad home access by default.
- No capsule gets Docker socket access by default.
- Windows capsule cannot write outside its prefix/export bridge.
- Capsule dependencies are supply-chain objects with signatures/SBOM/provenance
  where available.
- Capsule rollback must include code, dependency layers, prefix/registry,
  config, and declared state migration.
- Capsule deletion must offer: app only, app + config, app + all data.
- Capsule breakouts become high-severity evidence.

#### Capsule reliability contract

"Works reliably" must mean:

```text
reproducible
+ pinned
+ health-checked
+ repairable
+ rollbackable
+ explainable when blocked
```

It cannot mean "every Windows/Android/Linux app always runs", because anti-cheat,
DRM, vendor drivers, kernel modules, licenses, and broken upstream software can
still block execution. AIOS should make failure rare, bounded, and honest.

Required subsystems:

| Subsystem | Purpose |
| --------- | ------- |
| Capsule Solver | Chooses capsule type, runner, dependency layers, sandbox, device policy, and fallback. |
| Compatibility Knowledge Base | Stores known-good recipes, runner versions, GPU quirks, codec needs, anti-cheat status, regressions. |
| Runner Registry | Versioned Wine/Proton/Wine-GE/Proton-GE/Linux runtime/container/VM/Wasm runners with signatures. |
| Dependency Recipe Store | Reproducible recipes for vcredist, dotnet, d3dx, fonts, codecs, VC runtimes, Java, Electron, Python, etc. |
| First Launch Probe | Runs app in observation mode and records missing DLLs, registry writes, network calls, GPU/audio init, crashes. |
| Health Check Engine | Per-app checks: executable starts, window appears, audio works, GPU path valid, save path writable, network reachable. |
| Snapshot Manager | Creates rollback points before runner/dependency/app/config/prefix changes. |
| Capsule Doctor | Repairs missing deps, broken prefix, runner mismatch, bad registry state, GPU path, fonts, codecs, file bridge. |
| Migration Engine | Moves capsule state across runner/app versions with pre/post validation. |
| Fallback Planner | Falls back from native to Flatpak/Nix/container/Windows capsule/VM/cloud/blocked-with-reason. |

#### Reliability state machine

```text
CREATED
  -> SOLVED
  -> PROBED
  -> HEALTHY
  -> DEGRADED
  -> REPAIRING
  -> HEALTHY
  -> or ROLLED_BACK
  -> or QUARANTINED
  -> or BLOCKED_WITH_REASON
```

State rules:

- `HEALTHY` requires a recent successful health check.
- `DEGRADED` means app can run but one declared feature is broken.
- `REPAIRING` must snapshot first.
- `ROLLED_BACK` returns code, deps, runner, prefix/config, and declared state to
  the previous known-good set.
- `QUARANTINED` preserves data and evidence while preventing launch.
- `BLOCKED_WITH_REASON` must name the blocker and safe alternatives.

#### Pinning and reproducibility

Each capsule should pin:

| Item | Why |
| ---- | --- |
| App artifact hash | Avoid silent upstream replacement. |
| Runtime/runner version | Wine/Proton/container/runtime updates can regress apps. |
| Dependency recipe versions | vcredist/dotnet/d3dx/font changes can break old apps. |
| GPU/video runtime path | DXVK/VKD3D/VA-API/NVENC changes can break rendering or capture. |
| Locale/font/input settings | Legacy apps often depend on locale, fonts, IME, keyboard layout. |
| File bridge rules | Avoid accidental broad data exposure after update. |
| Network/service endpoints | Detect vendor endpoint drift and suspicious new egress. |

The system may recommend updates, but the capsule stays on its last known-good
runtime until the update passes compatibility checks.

#### Windows reliability additions

Windows capsules need extra logic because the app usually expects a full Windows
machine and mutable global state.

| Area | Required AIOS behavior |
| ---- | ---------------------- |
| Prefix architecture | Decide `win32` vs `win64` once; changing it creates a new capsule lineage. |
| Runner selection | Score Wine, Proton, Wine-GE, Proton-GE, vendor-known runner; pin selected runner. |
| DLL detection | Parse crash/log output and known recipes to suggest/install missing DLLs. |
| Registry diff | Snapshot registry before/after install, first launch, dependency install, and update. |
| Installer capture | Observe MSI/EXE installers in App Lab before promoting prefix changes. |
| Redistributables | Manage vcredist/dotnet/msxml/d3dx/d3dcompiler/media/font dependencies as signed recipes where legally distributable. |
| Graphics | Detect DirectX version, choose DXVK/VKD3D/OpenGL, test GPU path, keep shader cache per capsule. |
| Audio | Validate PipeWire bridge, sample rate/latency, microphone/capture permission. |
| Fonts | Bundle required fonts when legal; otherwise explain missing-font risk. |
| File dialogs | Prefer portal/export bridge over full home mapping. |
| Save data | Discover save paths and attach backup/sync/rollback policy. |
| Launchers | Treat launchers as parent apps with child app/game capsules, not as unrestricted package managers. |
| Anti-cheat/DRM | Record truth: supported, unknown, blocked, vendor-refuses, VM-hostile. |

#### Capsule update strategy

Updates are staged, never directly applied:

```text
current known-good capsule
  -> create update candidate
  -> apply to clone
  -> run install/launch/health checks
  -> compare new capabilities
  -> operator approval if risk changed
  -> promote candidate
  -> keep previous rollback snapshot
```

Update blockers:

- new broad filesystem access
- new background service
- new privileged helper
- new network endpoints
- runner or dependency regression
- GPU/audio/video failure
- data migration failure
- unsigned or unverifiable artifact

#### Data safety requirements

Capsules must never make user data hostage.

Required operations:

- export user-created documents
- export/import capsule
- backup/restore app state
- copy save games separately from app code
- reset app while preserving user data
- reset prefix while preserving save data where possible
- wipe secrets and tokens without deleting documents
- show exactly what "delete app" will remove

#### Performance requirements

Capsules must not become too heavy to use daily:

| Requirement | Target behavior |
| ----------- | --------------- |
| Shared runtime layers | Thin capsules reuse signed shared layers. |
| Lazy dependency fetch | Download/install dependencies only when selected recipe needs them. |
| Content-addressed cache | Same dependency is stored once across capsules. |
| Precompiled shader cache | Games keep per-capsule shader cache with size limit and cleanup. |
| Resource budgets | CPU/RAM/GPU/disk/network budgets per workspace/profile. |
| Fast launch path | Healthy capsules skip expensive probes unless runner/deps/system changed. |

#### Observability and support

Every capsule needs a support bundle:

```text
CapsuleSupportBundle
  manifest
  app passport
  runner/dependency lockfiles
  last health check
  crash summaries
  redacted logs
  policy denials
  GPU/audio/video diagnostics
  network denials
  rollback points
```

This lets AIOS answer:

- why does this not start?
- what changed since it last worked?
- which dependency is missing?
- which permission was denied?
- did the runner update break it?
- can we roll back safely?
- should we route this app to VM fallback?

#### "No surprises" operator rules

- Before first launch, show what the capsule can touch.
- Before update, show what changes.
- Before repair, snapshot.
- Before deleting, show app/config/state/cache/saves separately.
- Before granting GPU/camera/mic/screen/USB, ask with exact reason.
- If the app cannot run safely, do not hide it: mark `BLOCKED_WITH_REASON`.

#### Product ideas

1. **AIOS App Capsule**
   - The user-visible unit for installing, running, moving, backing up, and
     deleting software.

2. **AIOS Windows Capsule**
   - One Windows app/game gets its own runner, prefix, dependencies, registry,
     graphics/audio/video policy, and save-state handling.

3. **AIOS Dependency Recipe**
   - Reproducible recipe for vcredist/dotnet/d3dx/fonts/media components and
     app-specific tweaks.

4. **AIOS Capsule Doctor**
   - Repairs broken dependencies, runner mismatch, registry drift, missing
     codecs, GPU path, font issues, and file bridge problems.

5. **AIOS Capsule Export**
   - Move one app with its capsule to another AIOS machine or airgap mirror,
     with signatures and compatibility notes.

Materialized Rev.3 contracts created on 2026-05-28:

- [`S17 App Capsule Runtime`](S17_App_Capsule_Runtime/00_overview.md)
- [`S17.1 Capsule Object Model`](S17_App_Capsule_Runtime/01_capsule_object_model.md)
- [`S17.2 Capsule Solver and Lifecycle`](S17_App_Capsule_Runtime/02_capsule_solver_lifecycle.md)
- [`S17.3 Windows Capsule Runtime`](S17_App_Capsule_Runtime/03_windows_capsule_runtime.md)
- [`S17.4 Reliability, Security, and Evidence`](S17_App_Capsule_Runtime/04_reliability_security_evidence.md)
- [`S17.5 Operator UI and Acceptance`](S17_App_Capsule_Runtime/05_operator_ui_acceptance.md)
- [`S18 Kernel Personality and Portability Plane`](S18_Kernel_Personality_Portability/00_overview.md)
- [`S19 Driver and Firmware Capsule Plane`](S19_Driver_Firmware_Capsule_Plane/00_overview.md)
- [`S20 Native AI Control Plane and AI Terminal`](S20_Native_AI_Control_Plane_Terminal/00_overview.md)

### Repo trust firewall

Adding every known repo must not mean trusting every repo equally.

Trust levels:

| Trust level | Example |
| ----------- | ------- |
| `AIOS_ROOT` | AIOS system packages, invariant bundles, recovery packages. |
| `VERIFIED_UPSTREAM` | Major distro official repos, verified Flathub publishers, signed vendor repos. |
| `COMMUNITY_REVIEWED` | Popular community packages with reproducible builds and clean history. |
| `UNTRUSTED_COMMUNITY` | AUR-like recipes, random GitHub releases, unsigned AppImages. |
| `QUARANTINED` | Known malicious, capability-lie, revoked, abandoned, or compromised sources. |

Rules:

- untrusted packages can be observed and converted, not blindly installed
- system mutation requires high trust + human approval
- repo scripts never get direct root
- every new repo has an evidence-backed onboarding event

### Script decompiler

Foreign package scripts are a major risk. Rev.3 should add a script decompiler:

```text
shell/scriptlet action:
  mkdir /opt/foo
  cp service /etc/systemd/system/foo.service
  systemctl enable foo

AIOS typed translation:
  filesystem.create_dir(scope=app_private)
  service.install_unit(unit_hash=...)
  service.enable(unit_id=foo.service, approval=required)
```

If a script cannot be translated safely, the package is blocked or routed to a VM.

### Rollback everything

Rollback should include:

- package files
- dependency graph
- generated config
- systemd units/timers/sockets
- dbus policy
- udev rules
- SELinux modules/labels
- firewall/network grants
- desktop entries
- file associations
- user state migration

No package install should be considered complete unless rollback is known.

### Dependency quarantine

Dependencies are not automatically innocent.

Rev.3 should score every dependency:

- source repo trust
- publisher trust
- known CVEs
- maintainer-script behavior
- transitive network capability
- native code / FFI / setuid bits
- kernel module / firmware request

Suspicious dependencies can be:

- replaced by safer provider
- isolated in container
- pinned to older safe version
- blocked
- routed to VM fallback

### Compatibility score before install

The marketplace should predict:

```text
Compatibility: 82%
Recommended runtime: Flatpak-style sandbox
Risk: medium
Reason:
  - package has background service
  - no SELinux profile upstream
  - official repo signature OK
  - no kernel module required
```

The score must be explainable and evidence-backed.

### Workspace worlds

AIOS should support isolated software worlds:

| Workspace | Profile |
| --------- | ------- |
| Work | Strict network, no games, high evidence retention. |
| Gaming | GPU-focused, Proton/Windows compatibility, no access to work/home secrets. |
| Lab | Risky packages allowed only in disposable sandboxes. |
| Family | Simple approvals, curated app set. |
| Admin | Recovery/policy/vault tools only; strong approval. |

Each workspace has separate apps, secrets, network, browser state, GPU budgets,
and package trust defaults.

### Secure Gaming Mode

Gaming is a major compatibility driver. Rev.3 should add:

- Proton/Wine per-game prefix isolation
- Steam/Epic/GOG/Heroic/Lutris import as typed app sources
- GPU class grants per game
- shader cache isolation
- controller/input broker
- save-state backup
- network allowlist per game/platform
- anti-cheat/DRM honesty classification
- VM fallback for kernel anti-cheat or hard DRM

Gaming mode must never read work/family/private data by default.

### Driver safety plane

Drivers, DKMS, kernel modules, firmware, GPU stacks, VPN kernel modules, and
filesystem drivers are a separate high-risk path.

Rules:

- recovery approval for kernel-affecting install
- signed module only by default
- out-of-tree driver gets high-risk warning
- TPM/Secure Boot/IMA evidence after install
- automatic rollback if boot drift detected
- module load evidence forever

### Mobile phone interface

Rev.3 should develop mobile in two tracks:

| Track | Meaning |
| ----- | ------- |
| `AIOS_MOBILE_RENDERER` | Phone app/web/PWA as control surface for desktop/server AIOS. |
| `AIOS_PHONE_EDITION` | AIOS running on phone-class hardware with Plasma Mobile/phosh-style shell. |

Priority should be `AIOS_MOBILE_RENDERER` first because it strengthens the
desktop/server product immediately.

Mobile renderer functions:

- approve package installs
- approve public network exposure
- approve firmware/kernel changes
- approve vault unlocks
- see install risk diff
- see package passport
- receive security alerts
- view evidence receipts
- emergency stop/quarantine app
- recovery pairing via QR code
- offline approval token when LAN is down

### Phone as root approval device

The phone should be usable as a hardware-adjacent approval console:

```text
desktop action requests high-risk permission
  -> phone receives signed approval request
  -> operator sees exact risk diff
  -> approval signs exact action hash
  -> desktop executes only if hash matches
```

This fits S5.3 `EXACT_ACTION` approval and strengthens the human-in-the-loop
boundary.

### Mobile continuity

Useful continuity features:

- start action on desktop, approve on phone
- monitor long installs from phone
- kill/quarantine app from phone
- receive "why blocked" explanation on phone
- scan QR on recovery console to pair emergency session
- carry a vault shard / recovery credential on phone

### AIOS Pocket Node

Future phone edition idea:

- phone is a small AIOS node
- carries local vault shard
- receives replicated evidence summaries
- can approve fleet actions
- can act as emergency recovery key
- can sync selected workspaces
- can run local low-power AI tasks

This should be deferred until the mobile renderer proves useful.

### Offline / airgap app store

For secure environments:

- signed repo snapshot on USB/NAS
- no live internet required
- package trust evaluated locally
- SBOM/provenance stored with snapshot
- update set approved as a batch
- evidence export for offline audit

This pairs with `AIRGAP_HIGH` security profile.

### Personal software mirror

Operator can maintain a personal signed mirror:

```text
upstream repos -> AIOS intake -> local mirror -> operator machines
```

Benefits:

- reproducible installs across all machines
- one audit per package version
- local cache for slow/offline networks
- deplatform/quarantine propagates to fleet
- no random machine fetches directly from internet

### Fleet mode

Rev.3 federation should include small-fleet software governance:

- one policy baseline across desktop/laptop/server/phone
- per-host override with evidence
- package passports synced
- install approvals replicated
- health and drift visible from one console
- local host remains sovereign; cluster root does not silently bypass host policy

### Why-is-this-blocked engine

When something fails, AIOS should explain the real blocker:

| Blocker | Example |
| ------- | ------- |
| SELinux | `aios_agent_t` denied write to package system path. |
| Policy | AI subject cannot initiate install. |
| Network | Endpoint outside manifest. |
| Sandbox | Package wanted broad home read. |
| Trust | Repo is `UNTRUSTED_COMMUNITY`. |
| Hardware | GPU class unavailable or denied. |
| Integrity | Package hash mismatch or unsigned artifact. |
| Compatibility | Runtime cannot support DRM/anti-cheat. |

This explanation should be produced from policy/evidence, not guessed by the UI.

### Product headline candidates

- **AIOS Universal Compatibility Plane**
- **AIOS Package Rosetta**
- **AIOS App Passport**
- **AIOS Mobile Approval Console**
- **AIOS Personal Software Mirror**
- **AIOS Secure Gaming Mode**

Strongest Rev.3 bundle:

1. Universal App Lab
2. Shadow Install
3. Install Risk Diff
4. Package Passport
5. Package Rosetta
6. Personal Software Mirror
7. Mobile Approval Console
8. Secure Gaming Mode

---

## Technical feasibility audit + native container/cloud runtime plane

Operator question on 2026-05-28:

> Is there a technical solution for everything requested? Add native Podman,
> Docker, maybe Kubernetes. What else is worth making native?

Short answer:

> Yes, every requested direction has a technical architecture path. No, AIOS
> should not promise that every package, game, driver, RT workload, anti-cheat,
> codec, or hardware device will always work directly on the host. The correct
> product promise is: every workload gets a best-fit execution plan, rollback,
> evidence, and a clear reason when blocked.

Design rule:

```text
request
  -> capability probe
  -> trust/risk decision
  -> native path if safe
  -> isolated/container path if needed
  -> VM/microVM/RT/island fallback if needed
  -> blocked with typed reason if impossible or unsafe
```

### Feasibility matrix

| Requested direction | Technical solution exists? | Rev.3 implementation path | Hard limit / honest caveat |
| ------------------- | -------------------------- | ------------------------- | -------------------------- |
| SELinux secure base | Yes. | SELinux domains, MCS/MLS labels where useful, policy evidence, audit export. | Certification requires external process; do not claim official compliance before audit. |
| Military/STIG-grade baseline | Yes as alignment. | DISA STIG/CIS/NIST mapping, OpenSCAP-style checks, hardening profiles. | "Aligned with" is realistic; "certified military OS" is not automatic. |
| Package-agnostic distro | Yes as an intake/decision engine. | Package Rosetta, app passport, shadow install, Nix/container/native/VM fallback. | Unknown scripts and privileged packages cannot be trusted blindly. |
| Mobile interface | Yes. | Adaptive renderer, phone approval, mobile fleet console, continuity. | Phone UI must be policy surface, not a second uncontrolled admin plane. |
| Dual normal/RTOS mode | Yes with profiles. | PREEMPT_RT profile, RT appliance boot, RT island using MCU/SoC/VM, evidence. | Hard real-time cannot be guaranteed on arbitrary consumer hardware. |
| Workstation-agnostic OS | Yes. | Workstation Passport, hardware fit checker, profile switcher, display/peripheral policy. | Vendor firmware and closed drivers still decide some outcomes. |
| Game-agnostic OS | Yes as runtime/source agnostic. | Game Passport, Proton/native/emulator/cloud/VM selector, anti-cheat honesty. | DRM and kernel anti-cheat may refuse Linux or VM execution. |
| Super video support | Yes. | Video Passport, PipeWire/codec/GPU probes, encode/decode/capture/stream scheduler. | Codec patents, vendor drivers, and hardware encode blocks are real constraints. |
| Native containers/cloud | Yes. | Podman, Docker compatibility, containerd/CRI-O, Kubernetes profiles, policy and evidence. | Docker socket and privileged containers must not become a root bypass. |

### Native container/cloud baseline

AIOS should treat containers as a first-class OS capability, not an add-on
package installed later by the operator.

Primary standards and sources:

| Layer | Native AIOS support |
| ----- | ------------------- |
| OCI standards | Native support for OCI image, runtime, and distribution concepts. OCI is the base contract for images and runtimes. Reference: [Open Container Initiative](https://opencontainers.org/). |
| Podman | Default secure local container engine, especially rootless containers, pods, and systemd Quadlet units. References: [Podman docs](https://podman.io/docs), [Podman Quadlet/systemd units](https://docs.podman.io/en/latest/markdown/podman-systemd.unit.5.html). |
| Docker Engine | Compatibility lane for existing Docker workflows, Compose stacks, CI examples, and vendor docs. Reference: [Docker Engine docs](https://docs.docker.com/engine/). |
| BuildKit | Native image build engine for fast, reproducible builds. Reference: [Docker BuildKit docs](https://docs.docker.com/build/buildkit/). |
| Compose spec | Native parser/importer for `compose.yaml`, independent of whether runtime is Podman or Docker. Reference: [Compose Specification](https://compose-spec.github.io/compose-spec/spec.html). |
| containerd | System/container runtime substrate and primary Kubernetes CRI option. Reference: [containerd](https://containerd.io/). |
| CRI-O | Kubernetes-focused alternate CRI runtime. Reference: [CRI-O CNCF](https://www.cncf.io/projects/cri-o/). |
| Kubernetes CRI | AIOS Kubernetes profile must speak Kubernetes CRI correctly; Kubernetes expects a CRI runtime on each node. Reference: [Kubernetes CRI](https://kubernetes.io/docs/concepts/architecture/cri/). |
| Local Kubernetes | First-class dev/edge/local clusters using a small profile such as K3s/k0s, plus dev-only Minikube/kind where useful. References: [K3s](https://docs.k3s.io/), [k0s](https://k0sproject.io/), [minikube](https://minikube.sigs.k8s.io/docs/). |

Recommended engine policy:

| Use case | Default choice | Reason |
| -------- | -------------- | ------ |
| Desktop app container | Podman rootless | No always-on root daemon; better fit for secure workstation profiles. |
| Existing Docker project | Docker compatibility or Podman compatibility | Maximize developer compatibility without making Docker socket universal root. |
| System service container | Podman Quadlet or managed containerd service | Integrates with systemd lifecycle and policy evidence. |
| Kubernetes node | containerd or CRI-O | Kubernetes-native CRI path; do not depend on Docker as kubelet runtime. |
| Image build | BuildKit plus Podman/Buildah path | Fast builds, reproducible cache, rootless build option. |
| Untrusted workload | gVisor/Kata/microVM/VM path | Containers share a kernel; stronger isolation needs another boundary. |

### Native Kubernetes platform profile

AIOS should support Kubernetes, but not force Kubernetes onto every desktop
profile. Kubernetes belongs in explicit profiles:

| Profile | Description |
| ------- | ----------- |
| `K8S_DEV_LOCAL` | Local single-node dev cluster for developers and labs. |
| `K8S_EDGE_NODE` | Lightweight edge node for homelab, IoT, small office, branch site. |
| `K8S_WORKSTATION_NODE` | Workstation can run local services, AI workloads, build pipelines, test clusters. |
| `K8S_SERVER_CLUSTER` | Real server/cluster install with HA, storage, ingress, policy, backup. |
| `K8S_AIRGAP_CLUSTER` | Offline cluster with signed local mirror and controlled updates. |
| `K8S_GPU_AI_NODE` | GPU-aware node for AI/media/render/game-stream workloads. |
| `K8S_RT_EDGE_NODE` | Experimental mixed-criticality/RT-adjacent edge profile with strict admission. |

Native Kubernetes support should include:

| Capability | Why it matters |
| ---------- | -------------- |
| `kubectl` context manager | Operators must see which cluster/context is active before commands execute. |
| Helm support | Many Kubernetes apps ship as Helm charts. Reference: [Helm](https://helm.sh/). |
| Kustomize support | Native Kubernetes configuration overlays without templating everything. Reference: [Kustomize](https://kustomize.io/). |
| GitOps controller profile | Flux or Argo CD for desired-state reconciliation. References: [Flux CNCF](https://www.cncf.io/projects/flux/), [Argo CNCF](https://www.cncf.io/announcements/2022/12/06/the-cloud-native-computing-foundation-announces-argo-has-graduated/). |
| CNI/network policy profile | Cilium as high-end eBPF networking/security/observability option. Reference: [Cilium](https://cilium.io/). |
| Admission policy | Kyverno and/or OPA Gatekeeper for policy-as-code. References: [Kyverno CNCF](https://www.cncf.io/projects/kyverno/), [OPA Kubernetes ecosystem](https://www.openpolicyagent.org/ecosystem/by-feature/kubernetes). |
| Runtime security | Falco/Tetragon-style runtime detection for hosts, containers, Kubernetes. Reference: [Falco](https://falco.org/). |
| Observability | OpenTelemetry, Prometheus-compatible metrics, logs, traces, event evidence. Reference: [OpenTelemetry](https://opentelemetry.io/docs/what-is-opentelemetry/). |
| Backup/restore | Velero-style backup for cluster resources and persistent volumes. Reference: [Velero](https://velero.io/). |
| External secrets | Native integration with Vault/cloud/on-prem secret managers. Reference: [External Secrets Operator](https://external-secrets.io/). |
| Registry/mirror | Signed local OCI registry mirror for airgap/fleet/offline installs. |
| GPU/device plugins | GPU/video/AI workloads require explicit device policy, not broad `/dev` access. |

### Native isolation levels

Containers are useful, but they are not one single security answer. AIOS should
select isolation level per risk:

| Isolation level | Candidate technology | Use case |
| --------------- | -------------------- | -------- |
| Process sandbox | SELinux, namespaces, seccomp, Landlock where useful | Low-risk local apps. |
| Rootless container | Podman/rootless OCI | Normal app/service containers. |
| Standard container | containerd/CRI-O/runc | Kubernetes workloads with normal risk. |
| Syscall-sandboxed container | gVisor/runsc | Untrusted services that need Linux ABI but reduced host kernel exposure. Reference: [gVisor](https://gvisor.dev/). |
| Lightweight VM container | Kata Containers | Stronger tenant/workload isolation with container UX. Reference: [Kata Containers](https://katacontainers.io/). |
| Full VM | KVM/QEMU/KubeVirt | Legacy apps, unsafe apps, other OS workloads, strong compartment. Reference: [KubeVirt](https://kubevirt.io/). |
| WebAssembly | WASI/WasmEdge/wasmCloud | Small portable services, plugins, edge functions, safer applets. References: [WASI](https://wasi.dev/interfaces), [WasmEdge](https://wasmedge.org/), [wasmCloud](https://wasmcloud.com/docs/v2.0.0-rc.1/). |
| RT island | PREEMPT_RT/Zephyr/co-kernel/appliance boot | Deterministic workloads. |

### Native developer/workload formats worth supporting

These are worth making native because they reduce friction and fit the
package-agnostic goal:

| Native format/capability | Rev.3 treatment |
| ------------------------ | --------------- |
| `.devcontainer/devcontainer.json` | Import dev environments into AIOS Workstation/Dev profiles. Reference: [Dev Containers spec](https://github.com/devcontainers/spec). |
| `compose.yaml` | Convert to Podman, Docker, or Kubernetes plan with network/volume/secret risk diff. |
| `Dockerfile` / `Containerfile` | Build through policy-aware BuildKit/Podman/Buildah path. |
| OCI artifacts | Treat images, Helm charts, SBOMs, signatures, policy bundles, and AI model artifacts as signed artifacts. |
| Helm charts | Install only through policy-admission and values diff. |
| Kustomize overlays | Promote environment-specific config without uncontrolled mutation. |
| Kubernetes manifests | Preflight with schema validation, policy, image trust, network/device requirements. |
| Systemd Quadlets | Preferred local service container format for single-host services. |
| SBOM | Require SPDX/CycloneDX where available. References: [SPDX](https://spdx.dev/about/overview/), [CycloneDX](https://cyclonedx.org/capabilities). |
| Signatures/provenance | Sigstore/cosign and SLSA-style provenance for images and packages. References: [Sigstore](https://docs.sigstore.dev/), [SLSA](https://slsa.dev/spec/v1.2/about). |

### AIOS Cloud Native Passport

Every containerized workload should get a passport similar to packages and
games:

```text
CloudNativePassport
  source: git | registry | local | compose | helm | k8s-manifest
  artifacts: images, charts, manifests, SBOMs, signatures
  runtime: podman | docker | containerd | crio | gvisor | kata | vm | wasm
  privileges: rootless, capabilities, seccomp, SELinux type, devices
  network: ports, egress, DNS, service mesh, ingress
  storage: volumes, secrets, persistence class, backup policy
  supply_chain: signature, provenance, SBOM, vulnerabilities
  update_policy: pinned, rolling, auto, manual approval
  rollback: snapshot, previous image digest, previous manifest
  evidence: logs, metrics, traces, audit denials, policy decisions
```

### Product ideas

1. **AIOS Native Containers**
   - Podman/rootless first, Docker-compatible, OCI-native.
   - Operator sees containers as governed workloads, not random root processes.

2. **AIOS Kubernetes Mode**
   - One-click profile for local/edge/server Kubernetes.
   - Uses CRI runtime, network policy, observability, backup, and signed mirror.

3. **AIOS Compose Importer**
   - Reads `compose.yaml` and produces a risk diff:
     ports, volumes, secrets, privileged mode, devices, images, update policy.

4. **AIOS Workload Passport**
   - One evidence object for package, container, VM, game, RT, or Wasm workload.

5. **AIOS Container Firewall**
   - Per-container network/device/filesystem policy with SELinux/eBPF evidence.

6. **AIOS Airgap Registry**
   - Local signed registry/mirror for OCI images, packages, Helm charts, SBOMs,
     model artifacts, and policy bundles.

7. **AIOS Dev Environment Import**
   - Native support for Dev Containers, Compose, Nix, Dockerfile, GitHub/GitLab
     repositories, and language lockfiles.

8. **AIOS Secure Runtime Selector**
   - Chooses `rootless`, `runc`, `gVisor`, `Kata`, `VM`, or `Wasm` based on risk,
     hardware, performance, and operator policy.

### What should be native beyond Podman/Docker/Kubernetes

Recommended native Rev.3 list:

| Native area | Include |
| ----------- | ------- |
| Container engines | Podman, Docker compatibility, containerd, CRI-O. |
| Build engines | BuildKit, Buildah/Podman build, reproducible build cache. |
| Local orchestration | Compose spec, Quadlet, systemd service integration. |
| Kubernetes | kubectl, CRI runtime, CNI, Helm, Kustomize, GitOps, policy, backup. |
| Isolation | SELinux, seccomp, rootless, gVisor, Kata, KVM/KubeVirt, Wasm/WASI. |
| Supply chain | Sigstore/cosign, SLSA provenance, SBOM SPDX/CycloneDX, image scanning. |
| Registry | Local OCI mirror/registry, offline sync, signed promotion gates. |
| Observability | OpenTelemetry, metrics/logs/traces, audit/event evidence. |
| Runtime security | Falco/eBPF-style detection, container drift detection, network flow evidence. |
| Secrets | Host vault, Kubernetes external secrets, hardware-backed keys where possible. |
| GPU/video/AI | Device plugins, GPU container runtime policy, video encode/decode access gates. |
| Developer UX | Dev Containers, Compose importer, Nix/profile bridge, language lockfile import. |
| Edge/fleet | K3s/k0s profiles, airgap updates, fleet policy sync. |
| VM workloads | KubeVirt/libvirt path for apps that cannot or should not be containerized. |
| Wasm workloads | WASI/WasmEdge/wasmCloud path for portable plugins and edge functions. |

### Non-negotiable safety rules

- Docker socket is never exposed by default.
- Privileged containers require explicit human approval and evidence.
- Kubernetes admin kubeconfig is not silently shared with AI subjects.
- Rootless is the default for local containers.
- Images are pinned by digest for secure profiles.
- Unknown images run in stricter isolation until trusted.
- Compose/Helm/Kubernetes imports show a risk diff before execution.
- Secrets are mounted or fetched through policy; they are not sprayed into
  environment variables by default.
- GPU, USB, camera, microphone, and video encode devices require declared
  device intent.
- Every container/workload can be rolled back or removed with evidence.

### Architectural conclusion

The technically correct direction is:

```text
AIOS = Linux distribution
     + package-agnostic intake
     + OCI/container-native runtime plane
     + Kubernetes profile plane
     + VM/Wasm/RT fallback planes
     + SELinux/policy/evidence around everything
```

This makes the "dream Linux" realistic: not one package manager, not one
runtime, not one desktop assumption, but one governed OS surface that can run
many software ecosystems without letting them own the host.

---

## Deep research scan — "dream Linux" / "perfect distro" / user wishlist

Operator research request on 2026-05-28:

> Research "my dream Linux" and variations. People must already have ideas.

Scope checked:

- community threads around "dream Linux distro", "perfect Linux system", and
  "what Linux desktop needs"
- existing distro/product patterns that already implement pieces of the dream:
  NixOS, blendOS, Vanilla OS/apx, Distrobox, Fedora Atomic, Bazzite,
  Kicksecure, Qubes OS, GrapheneOS, older multimedia-focused Dreamlinux /
  Dream Studio patterns
- gaming ecosystem signals around Proton, Steam Deck/Bazzite, anti-cheat,
  compatibility databases

Representative sources:

| Source | Signal |
| ------ | ------ |
| [r/linuxquestions: dream distro](https://www.reddit.com/r/linuxquestions/comments/1qb70wk/if_you_could_design_your_dream_linux_distro_what/) | Users warn against "just another distro"; ask for a clear target, less choice paralysis, rollback, stability + freshness. |
| [r/linuxquestions: perfect system](https://www.reddit.com/r/linuxquestions/comments/13yx89t/what_is_the_perfect_system_to_you/) | Security-centric rolling desktop, OpenBSD/GrapheneOS-like hardening, privacy, daily usability. |
| [r/technology: Linux desktop vs Windows](https://www.reddit.com/r/technology/comments/1pvudm3/what_the_linux_desktop_really_needs_to_challenge/) | Mainstream users want GUI flows, less terminal, app/game compatibility, OEM support, less fragmentation pain. |
| [Network World: perfect Linux distro](https://www.networkworld.com/article/945744/what-would-the-perfect-linux-distro-look-like.html) | Wide software support, graphical package manager, tablet/touch scaling, minimal "invented here", easy remixing/redistribution. |
| [Distrobox](https://distrobox.it/) | Strong proof that users want "any distro userland on any host", but also a warning: tight host integration is not strong sandboxing. |
| [blendOS](https://blendos.co/) | Declarative/atomic base plus apps/binaries from multiple distros and Android. |
| [Nix / NixOS](https://nixos.org/guides/how-nix-works/) | Declarative config, atomic upgrades, safe test builds, fast rollback, reproducible machine state. |
| [Nix ecosystem](https://wiki.nixos.org/wiki/Nix_ecosystem/en) | Huge package/module ecosystem, but learning curve/documentation complexity remain pain points. |
| [Fedora Atomic Desktops](https://fedora.gitlab.io/ostree/docs/fedora-atomic-desktops/) | Image-based desktop with read-only root and standard Fedora behavior where possible. |
| [Bazzite](https://bazzite.gg/) | Gaming-focused atomic desktop with Steam, HDR/VRR, gaming tweaks, and newcomer-friendly defaults. |
| [Qubes OS](https://doc.qubes-os.org/en/latest/introduction/intro.html) | Security by compartmentalization: users want desktop workflows split by trust domain. |
| [Kicksecure](https://www.kicksecure.com/wiki/About) | Security-hardened Linux with many concrete hardening defaults. |

### Repeated public wishes

| Pattern | What people ask for | AIOS Rev.3 interpretation |
| ------- | ------------------- | ------------------------- |
| Less choice paralysis | "Which distro?" is confusing; users want one clear path. | AIOS should be profile-driven, not distro-hop-driven: Work, Gaming, Admin, Creator, RT, Airgap. |
| GUI-first operations | Users dislike needing terminal for drivers, fingerprint, package fixes, hardware config. | Every normal operator path needs renderer UI + "why blocked" explanation; CLI remains recovery/admin, not mandatory daily path. |
| App compatibility | Missing Windows/macOS/pro apps and fragmented package formats block adoption. | Package Rosetta + Universal App Lab + VM/compatibility fallbacks. |
| Game compatibility | Proton works well, but anti-cheat/DRM/store fragmentation still blocks people. | Game Passport + anti-cheat honesty + runtime selector + store adapters. |
| Hardware/drivers | Wi-Fi, GPU, fingerprint, suspend, video, hybrid graphics still make or break desktop Linux. | Workstation Passport + hardware fit checker + driver safety plane + video passport. |
| Atomic rollback | Users love "if update breaks, roll back in minutes." | Shadow install, rollback everything, image/profile promotion gates. |
| Security by default | Security-centric users want OpenBSD/GrapheneOS/Qubes-like discipline on Linux desktop. | SELinux/STIG profile + compartment/workspace model + phone approval. |
| Fresh but stable | Rolling freshness without Arch-style breakage; stable but not obsolete. | Channel model: stable base + fast app/runtime lanes + gated kernel/driver lanes. |
| Wide software source support | Users want packages from multiple ecosystems without wrecking host. | Package-agnostic intake, repo trust firewall, Nix/container/native/VM choices. |
| Polished media | Multimedia distros historically attracted users by making codecs/creation work out of the box. | Video/media acceleration plane, creator mode, codec/hardware diagnostics. |
| Privacy / no forced cloud | Users dislike unwanted online integration. | Network default-deny, app network manifests, no hidden cloud calls, evidence for outbound. |
| Easy remaster/customize | Some dream distro discussions want remix tools and redistributable profiles. | AIOS Profile/Image Builder: signed role images, not random respins. |

### Gaps in existing projects that AIOS can exploit

| Existing pattern | Strength | Gap AIOS can close |
| ---------------- | -------- | ------------------ |
| Distrobox/apx | Multi-distro app/userland access. | Not a security boundary by default; AIOS can add policy, SELinux, evidence, passport, rollback. |
| NixOS | Reproducibility, atomic rollback, declarative machine state. | High learning curve; AIOS can expose intent/profile UI and hide language complexity from operators. |
| Fedora Atomic / Bazzite | Read-only base, image updates, gaming-friendly variants. | Still package/runtime-specific; AIOS can add package-agnostic intake + trust firewall + policy evidence. |
| Qubes OS | Strong compartmentalization. | Heavy VM-first model; AIOS can offer graded isolation: SELinux/sandbox/container/VM per risk. |
| Kicksecure | Concrete Linux hardening defaults. | Not a universal compatibility/workstation/gaming platform; AIOS can merge hardening with app compatibility. |
| Gaming distros | Good default tweaks for Steam/Proton/GPU. | Often store/runtime-specific; AIOS can be game-source-agnostic and evidence-driven. |
| Old multimedia distros | Proved "media works out of the box" is a strong user desire. | Modern version needs hardware codec probes, PipeWire portals, creator pipelines, streaming, evidence. |

### Rev.3 research-backed product ideas

1. **AIOS Profile-First Desktop**
   - On first boot, choose role: Workstation, Gaming, Creator, Admin, Lab,
     Airgap, RT, Thin Client.
   - The profile sets security posture, package lanes, GPU/video policy, update
     cadence, workspace defaults.

2. **AIOS Guided Migration**
   - Import from Windows/macOS/Linux:
     apps, documents, browser profiles, game libraries, SSH keys, development
     environments, network printers, cloud accounts.
   - Each import becomes a typed plan with risk diff and rollback.

3. **AIOS App Store Without Fragmentation**
   - One search UI across AIOS packages, Flatpak, distro repos, Nix, AppImage,
     OCI images, Steam/GOG/Epic/itch, Android.
   - Results show recommended path, trust, runtime, risk, compatibility, rollback.

4. **AIOS "It Runs / It Does Not / Here Is Why" Contract**
   - Users hate ambiguous Linux breakage.
   - Every failure should produce a typed reason: driver, codec, anti-cheat,
     package trust, SELinux, portal, network, dependency, hardware.

5. **AIOS Clean System Guarantee**
   - Foreign packages and build scripts never dirty the base OS.
   - Shadow install, app passport, dependency snapshot, rollback everything.

6. **AIOS Human-Friendly Nix**
   - Use Nix-style reproducibility where it helps, but do not expose raw Nix
     language as the operator interface.
   - Profiles generate build/runtime plans; advanced users can export/edit.

7. **AIOS Security Workspaces**
   - Qubes-like idea, but graded:
     same-kernel SELinux workspace, container workspace, VM workspace, airgap.
   - Work, banking, gaming, lab, family, admin all have separate trust domains.

8. **AIOS Hardware Compatibility Oracle**
   - Before install or profile switch, tell the truth:
     GPU OK, Wi-Fi risky, fingerprint unsupported, video encode available,
     suspend verified, RT impossible, gaming good except anti-cheat titles.

9. **AIOS Polished Media/Desktop Experience**
   - Not just codecs installed.
   - Video passport, PipeWire portal health, HDR/VRR, screen share, webcam,
     conference, recording, creator pipeline, game streaming.

10. **AIOS Remix/Profile Builder**
    - Users and organizations can create signed profiles/images:
      "AIOS Secure Admin", "AIOS Classroom", "AIOS CAD", "AIOS Gaming", "AIOS
      Airgap Lab".
    - Profiles are data and policy, not forks of the distro.

### Research conclusion

The public "dream Linux" pattern is not one single UI or package manager. It is
the combination of:

```text
just works
+ broad software/game compatibility
+ safe rollback
+ no terminal for ordinary operations
+ strong security defaults
+ honest hardware/app compatibility
+ low fragmentation at the user surface
+ high customization behind a profile system
```

AIOS already has the right bones for this because Rev.2 separates policy,
evidence, sandbox, package trust, compatibility runtimes, renderers, hardware,
and network. Rev.3 should turn that into the operator-visible promise:

> One OS surface, many software ecosystems, many workstation roles, explicit
> security, clear explanations, reversible changes.

---

## Operator UI / UX expansion + modern UI technology stack

Operator question on 2026-05-28:

> Can the UI be developed further? Are there modern technologies worth using?

Short answer:

> Yes. UI is not just a theme. For AIOS it should become the operator surface
> for policy, packages, containers, Kubernetes, gaming, video, RT sessions,
> fleet, security, rollback, and mobile approval.

The UI should be treated as a governed renderer over typed system state, not as
a privileged app that mutates the OS directly.

```text
system state / evidence / policy
  -> SurfaceSchema
  -> renderer: desktop | web | mobile | TUI | kiosk | TV/game | voice
  -> signed operator action request
  -> policy decision
  -> execution by trusted backend
```

### Modern UI technologies worth using

| Technology | Why it is useful for AIOS | Recommended role |
| ---------- | ------------------------- | ---------------- |
| Wayland | Modern Linux display protocol and compositor model. | Default display/session foundation; X11 only as compatibility. |
| XDG Desktop Portals | Standard permission bridge for sandboxed apps: files, screen share, camera-like flows, desktop integration. Reference: [xdg-desktop-portal](https://flatpak.github.io/xdg-desktop-portal/). | Mandatory UI/security bridge for Flatpak-style apps, screen sharing, file pickers, portals. |
| KDE Plasma + Kirigami / Qt Quick | Convergent desktop/mobile UI framework; strong fit for desktop, phone, tablet, TV, handheld. Reference: [Kirigami](https://develop.kde.org/frameworks/kirigami/). | First-class AIOS desktop/mobile shell and native control center path. |
| GTK4/libadwaita | Adaptive Linux app model with strong GNOME ecosystem. Reference: [libadwaita adaptive layouts](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/adaptive-layouts.html). | Optional compatibility/native app lane, especially if GNOME renderer is added later. |
| Web/PWA renderer | Universal remote UI for localhost, server, fleet, phone browser, thin client. | AIOS web console, fleet board, remote workstation control. |
| Tauri 2 | Rust backend plus system WebView; smaller than Electron-style bundled browser apps. Reference: [Tauri](https://v2.tauri.app/). | Good for admin/control apps where web UI speed + Rust backend security matter. Do not use for the whole shell. |
| Slint | Modern Rust/C++/JS/Python UI toolkit for embedded, desktop, mobile, web. Reference: [Slint docs](https://docs.slint.dev/index.html). | Recovery UI, kiosk, appliance UI, RT panel, embedded/edge console. |
| AccessKit | Cross-platform accessibility abstraction for custom-rendered UI. Reference: [AccessKit](https://accesskit.dev/). | Required if AIOS builds custom Rust/GPU-rendered controls. |
| WebGPU / wgpu | Modern GPU graphics API for high-performance visualizations. References: [WebGPU](https://webgpu.org/), [wgpu](https://wgpu.rs/). | Use for topology maps, workload graphs, video/GPU dashboards, not ordinary forms. |
| PipeWire + portals | Modern audio/video/screen capture routing. | UI indicators and consent flows for camera, microphone, screen share, recording. |
| Design tokens | Single source for colors, spacing, typography, motion, density, accessibility. | One AIOS identity across desktop, mobile, web, kiosk, game mode. |

### UI surfaces to build in Rev.3

| Surface | Purpose |
| ------- | ------- |
| `AIOS_CONTROL_CENTER` | Main system panel: security, updates, packages, containers, hardware, video, network, storage. |
| `AIOS_PROFILE_SWITCHER` | Switch role profiles: Workstation, Gaming, Creator, Admin, Lab, Airgap, RT, Thin Client. |
| `AIOS_APP_LAB_UI` | Unified app/package store: distro packages, Flatpak, Nix, AppImage, OCI, Steam/GOG/Epic/itch, Android. |
| `AIOS_WORKLOAD_PASSPORT_VIEW` | One view for app/package/container/game/VM/Wasm/RT workload: trust, runtime, devices, network, rollback. |
| `AIOS_WHY_BLOCKED_PANEL` | Human-readable explanation when SELinux, policy, hardware, anti-cheat, codec, network, trust, or sandbox blocks an action. |
| `AIOS_SECURITY_MAP` | Visual trust domains: work, banking, gaming, admin, lab, family, airgap. |
| `AIOS_CONTAINER_K8S_BOARD` | Visual Compose/Kubernetes/container topology with ports, volumes, secrets, images, policies. |
| `AIOS_VIDEO_STUDIO` | Codec/GPU/camera/screen share/recording/streaming diagnostics and controls. |
| `AIOS_GAME_HUB` | Game libraries, runtime selector, Proton/native/emulator/cloud/VM status, anti-cheat warning. |
| `AIOS_RT_SESSION_CONSOLE` | RT workload fit check, CPU/IRQ isolation, latency evidence, start/stop approval. |
| `AIOS_FLEET_BOARD` | Multi-machine health, drift, updates, package passports, policy rollout. |
| `AIOS_MOBILE_APPROVAL` | Phone approval for high-risk installs, admin actions, camera/mic/screen/device permissions. |
| `AIOS_RECOVERY_TUI` | Keyboard-only recovery interface when graphics, web, or mobile renderer is unavailable. |
| `AIOS_KIOSK_BUILDER` | Signed kiosk/thin-client profiles for public, classroom, industrial, retail, operations screens. |

### Design direction

AIOS should not look like a generic consumer Linux theme. Recommended direction:

```text
industrial operator UI
+ calm security dashboard
+ high-density workstation controls
+ clear risk language
+ adaptive mobile approval
+ strong visual evidence
```

Practical rules:

- ordinary operations should not require terminal
- every destructive or high-risk action gets preview, diff, approval, rollback
- every security denial gets a human explanation
- UI shows current trust domain at all times
- camera, microphone, screen sharing, GPU/video encode, USB, and Kubernetes
  admin context always have visible status
- all surfaces must work with keyboard, touch, screen reader, high contrast, and
  reduced motion
- Bulgarian and English localization should be first-class, not an afterthought
- mobile layout starts from the smallest screen first, then scales up
- CLI/TUI remains mandatory for recovery, but not for daily use

### Core UX flows

#### App install

```text
search app
  -> AIOS shows install paths: native / Flatpak / Nix / container / VM
  -> risk diff: files, network, devices, scripts, trust, rollback
  -> operator approves
  -> install runs with evidence
  -> app passport appears
  -> rollback/remove is one action
```

#### Container / Kubernetes import

```text
open compose.yaml / Helm chart / Kubernetes manifest
  -> topology graph
  -> images + signatures + SBOM
  -> ports + volumes + secrets + devices
  -> policy violations
  -> choose Podman / Docker compatibility / local K8s / server K8s
  -> deploy with rollback
```

#### Blocked action

```text
action fails
  -> reason: SELinux | policy | trust | hardware | codec | anti-cheat | network
  -> exact denied object
  -> safe alternatives
  -> "request exception" only if policy allows
```

#### Phone approval

```text
desktop/server requests high-risk action
  -> phone receives signed request
  -> operator sees diff and device identity
  -> biometric/PIN approval
  -> host executes only matching signed action
```

### What not to do

- Do not make the UI a skin/theme project first. The value is workflow and
  evidence.
- Do not put a chatbot over everything and call it an interface.
- Do not make Electron-style bundled browser apps the system shell.
- Do not hide risk behind friendly wording.
- Do not give renderers direct root/admin authority.
- Do not build separate desktop/mobile/web products with different behavior.
  They must render the same SurfaceSchema.

### Product ideas

1. **AIOS Control Center**
   - One serious operator panel for the whole OS.

2. **AIOS Surface Graph**
   - Visual map of apps, containers, VMs, devices, network, trust zones, evidence.

3. **AIOS Profile Studio**
   - Build signed profiles: Secure Admin, Gaming, CAD, Classroom, Airgap Lab,
     Kiosk, RT Appliance.

4. **AIOS Action Diff**
   - Every install/update/deploy/admin action shows before/after state.

5. **AIOS Evidence Drawer**
   - Logs, policy decisions, SELinux denials, network flows, package hashes,
     rollout history in one place.

6. **AIOS Mobile Commander**
   - Phone as approval key, monitor, kill switch, and recovery helper.

7. **AIOS Recovery Console**
   - TUI/keyboard-only fallback that can repair boot, rollback, disable bad
     profiles, and export evidence.

8. **AIOS Workload Topology**
   - WebGPU/wgpu visualizer for containers, Kubernetes, VMs, services, ports,
     dependencies, and network policy.

### UI architecture conclusion

The strongest Rev.3 UI direction is:

```text
KDE/Wayland/Kirigami native shell
+ portal-first permission UX
+ web/Tauri remote admin console
+ Slint/Rust recovery and kiosk UI
+ WebGPU/wgpu for heavy visual maps
+ shared SurfaceSchema and design tokens
+ policy/evidence-driven action model
```

This turns AIOS from "a Linux distro with settings" into an operator-grade OS
with a consistent control surface across desktop, phone, browser, kiosk, gaming,
RT mode, and server/fleet use.

---

## Additional Rev.3 directions — RTOS, workstation-agnostic, game-agnostic

Operator direction on 2026-05-28:

> Three more directions: a dual-kernel system where one side handles normal
> business operations and another can act as RTOS when needed; make the
> distribution workstation-agnostic; make it game-agnostic.

External anchors for scoping:

| Area | Reference |
| ---- | --------- |
| Real-time Linux | [Linux kernel PREEMPT_RT documentation](https://www.kernel.org/doc/html/next/core-api/real-time/index.html) |
| Dual-kernel real-time Linux | [Xenomai Cobalt co-kernel documentation](https://doc.xenomai.org/v3/html/README.INSTALL/index.html) |
| Embedded RTOS option | [Zephyr Project RTOS documentation](https://docs.zephyrproject.org/latest/index.html) |
| Windows game compatibility | [Valve Proton](https://github.com/ValveSoftware/Proton) |
| Game compositor | [Valve gamescope](https://github.com/ValveSoftware/gamescope) |
| Non-Steam stores | [Heroic Games Launcher](https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher) |

### 1. Dual-kernel / RTOS-capable AIOS

Goal:

> AIOS remains a normal secure Linux workstation/server for business workloads,
> but can provide deterministic real-time execution for specific workloads when
> needed.

Important distinction:

- A normal Linux desktop should **not** be treated as a full RTOS.
- `PREEMPT_RT` makes Linux much better for bounded-latency workloads, but hard
  real-time guarantees still need careful hardware, IRQ, scheduler, and workload
  isolation.
- For true hard real-time, the safer architecture is an **RT island**: a
  co-kernel, isolated cores, RT VM, or a separate MCU/SoC running an RTOS.

Proposed modes:

| Mode | Description | Use case | Risk |
| ---- | ----------- | -------- | ---- |
| `LINUX_STANDARD` | Normal AIOS kernel profile. | Business, desktop, server, dev, browsing. | Lowest complexity. |
| `LINUX_PREEMPT_RT` | AIOS kernel built with real-time preemption profile. | Audio, robotics control UI, industrial soft RT, low-latency trading lab, measurement. | Needs tuning; not a blanket hard-RT guarantee. |
| `RT_CO_KERNEL` | Linux plus a real-time co-kernel layer such as Xenomai/Cobalt-style architecture. | Harder real-time workloads that need Linux side-by-side. | High complexity, hardware/driver constraints. |
| `RT_ISLAND_ZEPHYR` | Dedicated MCU/core/VM running Zephyr or another RTOS; Linux communicates over typed channels. | Sensors, motor control, deterministic device loops, safety companion. | Requires hardware split and protocol discipline. |
| `RT_APPLIANCE_BOOT` | Reboot into a minimal RT image, no normal desktop. | Dedicated measurement/control session. | Simple and safer, but not simultaneous with normal workstation mode. |

Recommended Rev.3 approach:

1. Start with `LINUX_PREEMPT_RT` as a **kernel profile**, not a separate product.
2. Add `RT_APPLIANCE_BOOT` for deterministic sessions that can tolerate reboot.
3. Add `RT_ISLAND_ZEPHYR` for hardware that has a suitable microcontroller,
   isolated core, or companion board.
4. Treat `RT_CO_KERNEL` as advanced/deferred until the kernel pipeline and
   hardware graph can prove support.

AIOS-specific control plane:

```text
normal AIOS action
  -> policy decision
  -> RT workload request
  -> hardware fit check
  -> CPU/IRQ/memory isolation plan
  -> operator approval
  -> RT session starts
  -> latency evidence emitted
  -> session teardown/rollback
```

Required contracts:

| Contract | Purpose |
| -------- | ------- |
| `RTWorkloadManifest` | Declares deadline, period, jitter budget, CPU affinity, memory lock, device needs, network needs. |
| `RTAdmissionController` | Refuses impossible RT claims before launch. |
| `RTLatencyEvidence` | Records cyclictest-like latency, missed deadline count, IRQ interference, CPU throttling. |
| `RTDeviceBinding` | Gives RT workload explicit, exclusive access to needed device paths/IRQs. |
| `RTTeardownPlan` | Returns CPU/IRQ/device state to normal AIOS after RT session. |

Real-time invariants:

- AI subjects cannot start an RT session without human approval.
- RT session cannot silently disable evidence, policy, SELinux, or recovery.
- RT workload gets only declared devices and CPU cores.
- Missed deadlines emit evidence; repeated misses degrade or stop the RT session.
- RT profile cannot be promoted unless latency tests pass on the target hardware.

Product ideas:

- **AIOS RT Mode** — operator-selectable mode for low-latency work.
- **RT Session Passport** — deadline, latency history, devices, CPU isolation,
  evidence, rollback.
- **RT Fit Checker** — "this laptop can/cannot run this workload deterministically."
- **RT Safety Gate** — refuses to run hard-real-time claims on unsuitable hardware.

### 2. Workstation-agnostic AIOS

Goal:

> AIOS should not be tied to one desktop shell, one hardware class, one GPU
> vendor, one display server shape, or one workstation layout.

This is different from package-agnostic. Package-agnostic means any software
source. Workstation-agnostic means any reasonable user machine shape.

Target workstation classes:

| Class | Example |
| ----- | ------- |
| `MOBILE_LAPTOP` | Battery, hybrid GPU, Wi-Fi, suspend/resume, mobile approval. |
| `DESKTOP_WORKSTATION` | Multi-monitor, discrete GPU, local storage, peripherals. |
| `DEV_WORKSTATION` | Containers, VMs, local clusters, compilers, source builds. |
| `CREATOR_WORKSTATION` | Audio/video, color profiles, GPU acceleration, tablet/stylus. |
| `CAD_ENGINEERING` | High GPU/VRAM, device passthrough, deterministic graphics stack. |
| `TRADING_OPERATIONS` | Multi-monitor, low-latency network, high audit, no surprise updates. |
| `SECURE_ADMIN_STATION` | Policy/vault/recovery console, strict network, no games/untrusted apps. |
| `THIN_CLIENT` | Mostly remote apps and streamed sessions. |
| `KIOSK_PUBLIC` | Locked app set, no package install, remote management. |
| `HEADLESS_WORKSTATION` | GPU/AI/render server controlled by web/mobile/CLI renderers. |

Renderer-agnostic model:

- KDE Plasma remains the richest first-class desktop renderer.
- Web renderer remains the universal localhost/remote renderer.
- CLI renderer remains recovery/admin minimum.
- Mobile renderer becomes approval/monitoring surface.
- Future GNOME/tiling/Wayland shells can bind to the same Surface/UI schema
  without owning authoritative state.

Hardware-agnostic model:

- GPU vendor abstraction: AMD, Intel, NVIDIA, software fallback.
- Input abstraction: keyboard/mouse/touch/stylus/gamepad/3D mouse.
- Display abstraction: single monitor, multi-monitor, HDR, VRR, projector, remote stream.
- Network abstraction: LAN, Wi-Fi, VPN, airgap, captive network, public exposure.
- Power abstraction: desktop, laptop battery, UPS, thermal throttle, quiet mode.

Workstation passport:

| Field | Meaning |
| ----- | ------- |
| Hardware class | Laptop/desktop/workstation/thin/kiosk/headless. |
| GPU profile | Vendor, driver, Vulkan/OpenGL/WebGPU support, VRAM, isolation state. |
| Display profile | Monitors, refresh, HDR, scaling, remote stream support. |
| Input profile | Touch, stylus, gamepad, hardware keys. |
| Power profile | Battery/AC/UPS, thermal budget, sleep policy. |
| Security profile | Dev/default/STIG/airgap/admin station. |
| Workload profile | Business, dev, creator, CAD, trading, gaming, RT. |

Workstation-agnostic features:

- **Workstation Fit Checker** — tells whether a machine can run a requested
  role safely and well.
- **Profile Switcher** — Work, Gaming, Lab, Admin, Creator, RT Mode.
- **Hardware Drift UX** — clear explanation when GPU/monitor/network/TPM changes.
- **Display Layout Passport** — multi-monitor layouts become signed objects with rollback.
- **Peripheral Policy** — USB, Bluetooth, HID, storage, camera, mic, gamepad all
  go through L8 hardware policy.
- **Remote Workstation Mode** — heavy workstation stays headless; phone/laptop/web
  renderers control it.

### 3. Game-agnostic AIOS

Goal:

> AIOS should not be Steam-only, Proton-only, Windows-game-only, or native-game-only.
> It should understand all major game sources and choose the best runtime per game.

Game sources:

| Source | AIOS handling |
| ------ | ------------- |
| Steam | Proton/native Steam runtime path; Steam metadata imported into game passport. |
| GOG | Native or Windows build; Heroic/Lutris-style source adapter; offline installer support. |
| Epic | Heroic/Legendary-style source adapter; account/token brokered through vault. |
| Amazon Games | Heroic-style source adapter where supported. |
| itch.io | Native, Wine, web, or sandboxed installer path. |
| Humble / standalone installers | Package Rosetta + game lab observation. |
| emulators | Emulator as runtime, ROM/media treated as user-owned content with legal disclosure. |
| browser/cloud games | Web runtime with controller/input/network policy. |
| Android games | Waydroid/Android VM path where viable. |
| Windows-only anti-cheat games | Honest classification: Proton-supported, VM-only, Windows-dual-boot-only, or blocked. |

Game runtime selector:

```text
game candidate
  -> source adapter
  -> metadata + compatibility profile
  -> runtime options:
       native Linux
       Steam Linux Runtime
       Proton stable/experimental/GE/UMU
       Wine
       Lutris-style recipe
       emulator runtime
       Android runtime
       Windows VM
       cloud/web
  -> benchmark/compatibility check
  -> recommended launch profile
```

Game passport:

| Field | Meaning |
| ----- | ------- |
| Store/source | Steam, GOG, Epic, itch, standalone, emulator, Android. |
| Runtime | Native, Proton, Wine, emulator, Android, VM, cloud. |
| Compatibility | Works, tweak needed, VM only, blocked, anti-cheat unsupported. |
| GPU profile | Vulkan/DXVK/VKD3D/OpenGL, VRAM budget, shader cache. |
| Input profile | Keyboard/mouse/gamepad/touch/gyro/VR. |
| Network | Multiplayer endpoints, voice, telemetry, LAN/public requirements. |
| Save state | Local path, cloud sync, backup/restore evidence. |
| Mods | Mod manager, mod trust, rollback, per-mod sandbox. |
| Performance | FPS/frametime/latency history per hardware profile. |

Game-agnostic features:

- **Per-game sandbox** — game cannot read business/work/family files.
- **Per-game GPU grant** — GPU class, VRAM budget, performance mode, thermal ceiling.
- **Per-game network manifest** — multiplayer/launcher/cloud-save endpoints only.
- **Shader cache passport** — shader cache versioned and per-game, with cleanup/rollback.
- **Cloud save broker** — store credentials stay in vault; game never sees raw secrets.
- **Mod sandbox** — mods are packages with trust, capability, rollback, and evidence.
- **Game performance lab** — run benchmark pass and record FPS/frametime evidence.
- **Anti-cheat honesty** — do not promise support where vendor blocks Linux/Proton.
- **Couch mode** — gamescope-like fullscreen session with AIOS security chrome above it.
- **Streaming mode** — local game streamed to phone/tablet/TV/web renderer.

Game-agnostic modes:

| Mode | Purpose |
| ---- | ------- |
| `SECURE_GAMING` | Normal safe gaming: sandbox, Proton/native, no personal data. |
| `PERFORMANCE_GAMING` | Higher GPU/CPU priority, explicit approval, thermal evidence. |
| `COMPATIBILITY_GAMING` | More permissive runtime for old games, still isolated. |
| `VM_GAMING` | Windows VM for hard compatibility cases. |
| `KIDS_GAMING` | Curated games, time/network limits, no store purchases by default. |
| `LAN_PARTY` | Temporary LAN exposure with TTL and evidence. |

Game-specific "why blocked":

- anti-cheat vendor blocks Proton/Linux
- game wants kernel driver
- DRM requires unsupported service
- network endpoint outside manifest
- store credential missing in vault
- GPU driver/capability mismatch
- mod requested unsafe filesystem write
- save path cannot be verified

Recommended Rev.3 treatment:

- Game-agnostic belongs under L6/L7/L8/L10.
- It should reuse package-agnostic intake, not fork a second installer.
- Games get a specialized passport and runtime selector because performance,
  GPU, input, mods, anti-cheat, and cloud saves are domain-specific.
- The first deliverable should be **Secure Gaming Mode** + **Game Passport** +
  **Steam/GOG/Epic source adapters**.

### 4. Super video / media acceleration plane

Operator note on 2026-05-28:

> For workstation-agnostic and game-agnostic AIOS, video support must be very
> strong.

Goal:

> AIOS should treat video as a first-class capability plane: playback, capture,
> conferencing, recording, streaming, remote desktop, game streaming, creator
> workflows, surveillance feeds, and mobile continuity should all use one
> policy/evidence-backed media model.

Technology anchors:

| Layer | Candidate foundation |
| ----- | -------------------- |
| Capture / routing | PipeWire for Linux audio/video graph and Wayland-safe screen capture. |
| Media pipelines | GStreamer for structured media graphs; FFmpeg/libav for codec tooling and conversion. |
| GPU video acceleration | VA-API, VDPAU where needed, NVIDIA NVENC/NVDEC, Intel QSV/VA, AMD VCN/VA, Vulkan Video where supported. |
| Display path | Wayland dmabuf, color management, HDR/VRR, fractional scaling, multi-monitor layout. |
| Streaming | WebRTC, SRT/RIST/RTMP where appropriate, LAN/local-first low-latency stream. |
| Camera stack | libcamera/portal-mediated camera access where available. |

Video capability classes:

| Capability | Meaning |
| ---------- | ------- |
| `VIDEO_PLAYBACK_BASIC` | Normal local playback, software fallback acceptable. |
| `VIDEO_PLAYBACK_HW` | Hardware decode required; records codec and device. |
| `VIDEO_ENCODE_BASIC` | Low-risk local transcode/recording. |
| `VIDEO_ENCODE_HW` | Hardware encode via NVENC/QSV/VA/Vulkan Video class backend. |
| `SCREEN_CAPTURE` | Screen/window capture through portal/PipeWire; explicit approval. |
| `CAMERA_CAPTURE` | Camera capture; explicit app/workspace permission. |
| `VIDEO_CONFERENCE` | Camera + mic + screen-share bundle with visible indicator. |
| `GAME_STREAMING` | Low-latency capture/encode/input-return path. |
| `REMOTE_DESKTOP_STREAM` | AIOS surface/session streaming to browser/phone/thin client. |
| `CREATOR_PIPELINE` | High-bitrate editing/transcode/color/HDR workflow. |
| `SURVEILLANCE_FEED` | Long-running camera/RTSP ingest with retention policy. |

Video passport:

| Field | Meaning |
| ----- | ------- |
| Codec support | H.264, H.265/HEVC, AV1, VP9, ProRes/DNxHR where relevant. |
| Decode path | Software, VA-API, QSV, NVDEC, VCN, Vulkan Video. |
| Encode path | Software, VA-API, QSV, NVENC, VCN, Vulkan Video. |
| Display features | HDR, VRR, color profile, scaling, multi-monitor support. |
| Capture path | PipeWire portal, camera portal, game capture, window capture. |
| Latency | Decode/encode/render latency, frame pacing, dropped frames. |
| Security | Which app can see camera/screen; visible indicator; evidence id. |
| Privacy | Recording retention, redaction, workspace boundary, group visibility. |
| Hardware fit | GPU/video engine availability and current contention. |

Video policy rules:

- Camera and screen capture always require visible UI indicator.
- AI subjects cannot request camera/screen capture silently.
- Screen capture must be window/surface-scoped by default, not full-desktop.
- Work workspace can forbid capture or watermark captured streams.
- Game workspace can allow low-latency game capture without granting home-file access.
- Admin/recovery surfaces can be marked non-capturable except by recovery-approved evidence capture.
- Video encode using hardware engines must obey GPU/video-engine budgets.
- Video recordings are evidence-aware: retention, redaction, and access class are explicit.

Video engine scheduler:

Rev.3 should add a scheduler for scarce video hardware resources:

```text
active video workloads:
  game stream encode
  video conference camera encode
  screen recording
  media transcode
  remote desktop stream

AIOS decides:
  priority
  codec
  hardware/software fallback
  bitrate/framerate/resolution cap
  thermal/power cap
  evidence retention
```

Priority examples:

| Workload | Default priority |
| -------- | ---------------- |
| Recovery/admin screen share | Highest, explicit approval. |
| Video conference | High, interactive. |
| Game streaming | High, low latency. |
| Remote workstation stream | Medium/high depending on session. |
| Background transcode | Low; pauseable. |
| Surveillance ingest | Steady; bounded retention. |

Workstation benefits:

- reliable screen sharing on Wayland
- predictable camera/mic permission model
- HDR/color profile aware creator workflows
- low-latency remote workstation streaming
- hardware acceleration diagnostics per GPU
- no black-box "why is video broken" debugging

Gaming benefits:

- gamescope-like fullscreen/couch session
- frame pacing and VRR evidence
- per-game capture and streaming profile
- shader/cache/video-engine interaction tracked
- cloud/remote play to phone/TV/browser
- controller input return path with latency evidence

Mobile benefits:

- phone as video approval console
- phone as remote display for workstation/session
- camera as explicit AIOS input device
- QR recovery pairing using camera
- low-bitrate admin stream over LAN/VPN

Why-is-video-broken engine:

| Blocker | Example |
| ------- | ------- |
| Codec | HEVC decode unsupported by available hardware/software policy. |
| Driver | GPU video engine absent, broken, or mismatched. |
| Policy | App lacks `SCREEN_CAPTURE` or `CAMERA_CAPTURE`. |
| Portal | PipeWire/desktop portal unavailable or denied. |
| Bandwidth | Remote stream cannot meet target bitrate/latency. |
| Thermal | Hardware encoder throttled; background transcode paused. |
| DRM | Protected content cannot be captured or streamed. |
| Workspace | Work/Admin profile forbids recording. |

Recommended Rev.3 deliverables:

1. **AIOS Video Passport**
2. **AIOS Media Capability Classes**
3. **PipeWire/portal capture contract**
4. **Hardware encode/decode probe**
5. **Video Engine Scheduler**
6. **Game Streaming Profile**
7. **Remote Workstation Stream**
8. **Why-is-video-broken diagnostics**

### Combined product shape

These three directions fit the existing architecture if treated as profiles and
capability planes, not as separate operating systems:

| Direction | Best Rev.3 shape |
| --------- | ---------------- |
| Dual-kernel / RTOS | `RTWorkloadManifest`, `RTProfile`, `RTSession`, latency evidence, optional RT island. |
| Workstation-agnostic | `WorkstationPassport`, hardware fit checker, renderer abstraction, role profiles. |
| Game-agnostic | `GamePassport`, game source adapters, runtime selector, secure gaming mode. |
| Super video support | `VideoPassport`, media capability classes, hardware codec probes, stream/capture policy. |

Strongest combined feature set:

1. **AIOS Workstation Passport**
2. **AIOS Role/Profile Switcher**
3. **AIOS RT Mode**
4. **AIOS RT Island Bridge**
5. **AIOS Game Passport**
6. **AIOS Game Runtime Selector**
7. **AIOS Secure Gaming Mode**
8. **AIOS Video Passport**
9. **AIOS Video Engine Scheduler**
10. **AIOS Why-Blocked Engine** for workstation/game/video/RT issues

---

## Technical compatibility analysis

The categories were analysed for architectural conflict. Result: **the 6 categories do not
contradict each other**. They split into two classes:

### Class A — Purely additive (≈ 40 of 43 sub-specs)

Add new contracts without changing existing ones. Zero risk to Rev.2 invariants.

- Whole category 1 — finishing scoped-out work
- Category 3: Time-as-plane + Energy + Backup/DR — new orthogonal layers
- Whole category 4 — extension on top of existing L5 CognitiveCore trait
- Whole category 5 — pure spec layer, no code
- Category 6: WASM + Deno + Bun + Python runtime adapters — 4 new `EcosystemRuntime` variants

### Class B — Require architectural revision (3 sub-specs)

Rev.2 invariants must be **extended, not replaced**.

| Topic                    | What changes                                                                                                                                                                                                 |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Multi-host cluster**   | Trust root becomes "host root → cluster root" (1 extra hop); evidence log becomes CRDT-replicated; INV-014 (append-only) is preserved because append-only continues to hold                                  |
| **AIOS-on-mobile**       | L1 substrate may be AOSP / Halium instead of mainline Linux; the other 10 layers stay; INV-001 (recovery boot without AI) must be re-phrased for a phone without physical keyboard recovery                  |
| **AIOS-on-edge minimal** | L5 CognitiveCore becomes **optional**; "AI proposes, never executes" still holds, but a **fallback path** is needed when no AI subject is present — typed `OperatorOnly` actions; affects S2.3 Policy Kernel |

---

## Technical obstacles per topic

| Topic                        | Obstacle                                                                                  | Resolvability                                                                                         |
| ---------------------------- | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| **TPM attestation**          | TPM 2.0 PCR set + remote attestation protocol (TCG NetEq) — non-trivial                   | Solved technology; `tpm2-tools` + `go-attestation` library                                            |
| **eBPF native runtime**      | In-kernel execution conflicts with "AI proposes, never executes" if opened to AI subjects | Constraint: eBPF runtime ALLOW only for HUMAN subjects; INV-002 extended with "AI cannot author eBPF" |
| **Federated identity**       | `SubjectId` today is `String`; federated must be `(home_realm, local_id)`                 | Backwards-compat shim: every local id becomes `realm:default:<id>`                                    |
| **Distributed evidence log** | BLAKE3 hash chain is linear; distributed needs Merkle DAG                                 | Known technique; chronicle / git-style append-only DAG                                                |
| **Multi-agent coordination** | Current `CognitiveCore` is single                                                         | Agent-as-Subject extension; each agent becomes an identifiable subject                                |
| **WireGuard cluster mesh**   | Pairwise peer config grows O(n²)                                                          | Hub-and-spoke or WireGuard mesh tools (innernet, headscale)                                           |

No item has a **fundamental blocker**. Everything is "we know how to do it, it is a matter of discipline."

---

## Control surfaces — where topics overlap

Top three intersection points across categories:

### 1. Trust root and TPM

- TPM attestation → changes L10 S11.1 §4 trust root chain
- Threshold root signing → also changes this
- Multi-host cluster → also

**Implication:** these 3 must be designed **together**, not separately. Single sub-spec.

### 2. Identity and federation

- Federated identity → `SubjectId`
- Cross-org trust delegation → `IdentityBundle`
- Multi-host cluster → realm-scoped identity

**Implication:** single sub-spec for the federated identity model.

### 3. Cognitive and multi-agent

- Multi-agent coordination → `CognitiveCore` becomes orchestrator
- Federated model marketplace → separate sub-spec for model distribution
- AI evaluation evidence → cross-cuts everything

**Implication:** Cognitive expansion as a cluster of 3 sub-specs designed together.

---

## A technically correct grouping (by dependencies, not timeline)

```text
Wave 1 (foundation):
  - Constitutional Hardening
      threshold root signing
      TPM attestation
      time-as-plane
  - Federated identity (required for Wave 2)

Wave 2 (federation):
  - Multi-host cluster
  - Cross-org trust delegation
  - Federated model marketplace
  - Distributed evidence log

Wave 3 (form factors):
  - AIOS-on-mobile
  - AIOS-on-edge minimal
  - L5 optional fallback

Wave 4 (depth + breadth):
  - Backup/DR plane
  - Energy policy
  - Multi-agent coordination
  - AI evaluation evidence

Wave 5 (runtimes + compliance):
  - WASM/eBPF/Deno/Bun/Python runtimes
  - GDPR + audit export
  - Voice/Mobile renderers
  - STRONG approval mechanics
```

This is the technically correct dependency order, ignoring calendar.

---

## Conclusion at this planning stage

**Everything is technically feasible.** No impossibility, no architectural conflict, no
gap in Rev.2 that blocks any of the topics. Only **3 of 43** require extension of
invariants; the rest are purely additive.

Rev.3 authoring will start **after** Rev.2 reaches FULL-REAL (M18 closed).

---

## Open questions to resolve before Wave 1 of Rev.3

These are not blockers, but they should be answered before writing any sub-spec:

1. **Cluster size assumption** — 2-3 hosts (homelab) or also 10-100 (small enterprise)?
   Pairwise WireGuard vs. mesh changes for large clusters.
2. **Mobile substrate** — Halium + mainline Wayland, or AOSP base, or pure mainline Linux
   phone (PinePhone / Librem 5 style)? Different L1 contract per choice.
3. **Hardware attestation root** — firmware-pinned (current) replaced by TPM, or
   firmware + TPM dual chain? Defence in depth vs. simplicity trade-off.
4. **eBPF subject set** — HUMAN only (safe), or also `_system` service subjects (useful
   for observability), or also AI with very narrow eBPF subset (`drop`-only, no `redirect`)?
5. **Compliance jurisdiction priority** — EU GDPR first, then SOC2, or US-first?
   Affects the order of sub-spec authoring.
6. **Voice renderer** — pure text-to-speech wrapper or full conversational? The latter
   needs a new L5 binding contract.
