# Rev.3 — Planning Notes

| Field       | Value                                                         |
| ----------- | ------------------------------------------------------------- |
| Status      | `PLANNING` (no sub-specs authored yet; Rev.2 still in-flight) |
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
