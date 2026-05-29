# S21 — Package Rosetta and Universal App Lab

| Field     | Value                                                                                                                                                                                                                                                                                                                     |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-29; awaiting implementation evidence)                                                                                                                                                                                                                                                         |
| Phase tag | S21                                                                                                                                                                                                                                                                                                                       |
| Layer     | Cross-cutting: L6 Apps/Packages/Compatibility and L10 Distribution/Ecosystem, crossing L2 AIOS-FS, L4 Policy/Identity/Vault, L9 Observability/Admin                                                                                                                                                                       |
| Consumes  | S12.1 App Runtime Model, S12.2 Package Model, S11.1 Repository Model and Trust Roots, S11.3 External Integrations Bridge, S17 App Capsule Runtime, S3.2 Sandbox Composition, S2.3 Policy Kernel, S5.3 Approval Mechanics, S3.1 Evidence Log, S9.1 Recovery Boundary                                                       |
| Produces  | `PackagePassport` (holistic §5 core state object), `PackageSource`, `PackageRosetta` pipeline, `SmartInstallDecision`, `RepoTrustLevel`, `ScriptDecompiler`, `ShadowInstall`, `InstallRiskDiff`, the Package Solver, Universal App Lab (S12.1 Phase A extension), package intake/rosetta/shadow/passport evidence records |

## 1. Responsibility

S21 owns the "package-agnostic distribution" promise. It is the single intake
plane that turns any known software-distribution format — Debian/RPM/Arch/Alpine
packages, Nix derivations, Flatpak/Snap/AppImage bundles, OCI images, source
releases, GitHub release assets, Windows installers, and Android packages — into
a governed AIOS object with explicit trust, capabilities, risk, rollback, and
evidence.

Packages are inputs. The `PackagePassport` is the operator-facing runtime truth.
S21 does not promise that every package runs natively; it promises that every
package receives a governed execution plan, a bounded blast radius, a rollback
path, an evidence chain, and — when it cannot be made safe — a clear blocked
reason.

S21 is deliberately not an app-execution engine. It produces the typed inputs
that S17 (App Capsule Runtime) consumes to materialize an `AppCapsule`. S21
catalogs, scores, translates, shadow-installs, and issues passports; S17 runs
capsules; S2.3 decides; S3.2 enforces; S3.1 records.

Invariant links: INV-002 (AI proposes, never executes), INV-004 (recovery
boundary), INV-008 (default-deny policy), INV-011 (cross-group access forbidden),
INV-013 (AI cannot perform system admin), INV-014 (no proof, no completion),
INV-017 (sandbox floor constitutional). New invariant proposed by S21:
**INV-029** (maintainer/foreign install scripts never execute as root or on the
live host — they are observed in an isolated lab and translated to typed
actions).

## 2. Product principle

```text
catalog everything
  -> trust selectively
  -> observe before install
  -> translate scripts into typed actions
  -> never run maintainer scripts as root
  -> shadow install off the live host
  -> render install risk diff
  -> policy + human approval
  -> promote with evidence and rollback, or block with reason
```

The default answer to "install this software" is never "run the vendor script as
root." The default answer is: identify the source, evaluate repo trust, observe
behavior in the Universal App Lab, translate every mutation into a typed AIOS
action, shadow-install off the active system, show the operator exactly what will
be touched, and promote only with policy approval and a known rollback.

## 3. Reference patterns

| Pattern                                                                                         | S21 use                                                                                                     |
| ----------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| [Debian maintainer scripts](https://www.debian.org/doc/debian-policy/ch-maintainerscripts.html) | `preinst`/`postinst`/`prerm`/`postrm` semantics that the Script Decompiler must observe, never blindly run. |
| [RPM scriptlets](https://docs.fedoraproject.org/en-US/packaging-guidelines/Scriptlets/)         | RPM `%pre`/`%post` scriptlet model and ordering for translation.                                            |
| [Arch PKGBUILD](https://wiki.archlinux.org/title/PKGBUILD)                                      | Untrusted build/install functions executed only in a hermetic build sandbox.                                |
| [Nix derivations](https://nixos.org/manual/nix/stable/language/derivations.html)                | Reproducible isolated environments; derivation hash recorded as provenance.                                 |
| [Flatpak manifest + portals](https://docs.flatpak.org/en/latest/manifests.html)                 | Permission/portal model mapped to AIOS capabilities.                                                        |
| [Snapcraft interfaces](https://snapcraft.io/docs/supported-interfaces)                          | Plugs/slots mapped to AIOS capabilities and sandbox profile.                                                |
| [AppImage format](https://docs.appimage.org/reference/appdir.html)                              | Extract-and-inspect path; no broad host execute by default.                                                 |
| [OCI image spec](https://github.com/opencontainers/image-spec)                                  | Container intake; Docker socket never exposed (per S11.3 bridge).                                           |
| [SLSA provenance](https://slsa.dev/spec/v1.0/provenance)                                        | Provenance attestation grade recorded in the passport.                                                      |
| [CycloneDX SBOM](https://cyclonedx.org/specification/overview/)                                 | SBOM ingestion/normalization for supply-chain risk scoring.                                                 |
| [SPDX](https://spdx.dev/specifications/)                                                        | License and component identification across ecosystems.                                                     |
| [OpenVEX](https://github.com/openvex/spec)                                                      | Vulnerability exploitability statements feeding the risk score.                                             |

## 4. Package source intake

Every artifact entering S21 is classified into exactly one `PackageSource` at the
intake step. The enum is closed.

```text
PackageSource =
  DEB_PACKAGE
| RPM_PACKAGE
| ARCH_PKGBUILD
| ALPINE_APK
| NIX_DERIVATION
| FLATPAK_BUNDLE
| SNAP_PACKAGE
| APPIMAGE_BUNDLE
| OCI_IMAGE
| SOURCE_RELEASE
| GITHUB_RELEASE
| WINDOWS_INSTALLER
| ANDROID_PACKAGE
| VM_IMAGE
```

Unknown values are rejected by the S21 intake loader; an artifact whose format
cannot be classified into one of these is recorded as a parse failure and routed
to the Universal App Lab as `unidentified`, never silently installed.

Default per-source handling (the intake loader emits this as a seed, not a final
decision — the Package Solver decides):

| `PackageSource`     | Default seed handling                                                                                  |
| ------------------- | ------------------------------------------------------------------------------------------------------ |
| `DEB_PACKAGE`       | Parse control + maintainer scripts + units; shadow install; convert to capsule or isolated native.     |
| `RPM_PACKAGE`       | Parse spec metadata + scriptlets + SELinux labels; shadow install; convert or native-isolated.         |
| `ARCH_PKGBUILD`     | Treat build/install functions as untrusted; hermetic build sandbox; translate intent to typed actions. |
| `ALPINE_APK`        | Lightweight native/container path for edge/headless profiles.                                          |
| `NIX_DERIVATION`    | Reproducible isolated env; record derivation hash as provenance.                                       |
| `FLATPAK_BUNDLE`    | Import manifest + portals; map permissions to AIOS capabilities.                                       |
| `SNAP_PACKAGE`      | Import plugs/slots; map to AIOS capabilities and sandbox profile.                                      |
| `APPIMAGE_BUNDLE`   | Extract, inspect, sandbox; never broad-host execute by default.                                        |
| `OCI_IMAGE`         | Run as container object with file/network/device policy; Docker socket never exposed.                  |
| `SOURCE_RELEASE`    | Build in hermetic builder; generate SBOM/provenance; emit AIOS package.                                |
| `GITHUB_RELEASE`    | Fetch signed asset if present; else untrusted/community path with high-friction approval.              |
| `WINDOWS_INSTALLER` | Route to S17 Windows capsule (Wine/Proton) or VM fallback; never mutate host.                          |
| `ANDROID_PACKAGE`   | Route to Waydroid/VM via S17; never adopt an AOSP base (DEC-R3-004).                                   |
| `VM_IMAGE`          | Run as a VM workload; full isolation; passport records VM runtime.                                     |

## 5. PackagePassport (core state object)

`PackagePassport` is a holistic §5 core state object and is defined
authoritatively here. It is the machine-readable, operator-facing truth surface
for any catalogued or installed software. AI and renderers MUST read the passport
before falling back to raw package metadata or shell output.

```yaml
package_passport:
  passport_id: "pkgpass_<ULID>"
  app_id: "app:<publisher>:<name>"
  schema: "aios.passport.v1alpha1"
  origin:
    source_kind: GITHUB_RELEASE # PackageSource enum
    repo_uri: "https://..."
    publisher: "publisher-id|unknown"
    package_format_version: "example"
    signature_chain: [] # empty = unsigned
    upstream_provenance_grade: NONE # ProvenanceGrade enum
  trust:
    repo_trust_level: UNTRUSTED_COMMUNITY # RepoTrustLevel enum
    publisher_reputation: "unknown|reviewed|verified|root"
    maintainer_reputation: "unknown|reviewed|verified"
    onboarding_evidence_id: "evr_..."
  runtime:
    selected_runtime: FLATPAK_STYLE # SmartInstallDecision enum value
    capsule_id: "appcap_<ULID>|none"
    isolation_floor: ISOLATED_SANDBOX # S3.2 floor; never weaker
  capabilities:
    filesystem_writes: [] # app-private scopes only by default
    network_outbound: [] # NetworkOutboundManifest endpoints
    gpu_class: NONE # NONE|GPU_COMPUTE|GPU_FULL_3D
    devices: []
    dbus_services: []
    portals: []
    secrets_requested: [] # always brokered, never raw (S2 Vault)
    requests_kernel_module: false
    requests_setuid: false
  risk:
    install_risk: LOW # LOW|MEDIUM|HIGH|CRITICAL
    runtime_risk: LOW
    supply_chain_risk: LOW
    sandbox_drift: NONE # NONE|MINOR|MAJOR
    sbom_ref: "evr_...|none"
    vex_ref: "evr_...|none"
    known_cve_count: 0
  evidence:
    intake_receipt: "evr_..."
    rosetta_receipt: "evr_..."
    shadow_install_receipt: "evr_..."
    last_install_receipt: "evr_..."
    last_launch_receipt: "evr_..."
    last_policy_decision: "evr_..."
  rollback:
    previous_passport_id: "pkgpass_...|none"
    dependency_snapshot_id: "snap_...|none"
    config_snapshot_id: "snap_...|none"
    rollback_plan_id: "rbk_..."
  compatibility:
    compatibility_score: 0 # 0-100, explainable + evidence-backed
    recommended_runtime: FLATPAK_STYLE
    known_issues: []
    blocked_reasons: [] # populated only when BLOCKED_WITH_REASON
```

Supporting closed enums introduced with the passport:

```text
ProvenanceGrade =
  NONE
| SIGNED_ASSET
| SLSA_L1
| SLSA_L2
| SLSA_L3
```

```text
RiskBand =
  LOW
| MEDIUM
| HIGH
| CRITICAL
```

Unknown values for any enum-typed passport field are rejected by the
`PackagePassport` validator. A passport with an unparseable enum is not issued;
the artifact stays catalogued-only.

Passport rules:

- A passport may exist for a catalogued-but-not-installed package; `runtime` and
  `evidence.last_install_receipt` are then unset.
- `isolation_floor` may never be weaker than the S3.2 runtime safety floor.
- `secrets_requested` entries are always satisfied through the Vault Broker as
  capabilities; raw secret material never appears in a passport.
- Every passport field that asserts behavior must trace to an evidence receipt;
  unverified assertions are rendered as "unknown," never as fact.

## 6. Repository trust firewall

Cataloging every known repository must not mean trusting every repository
equally. `RepoTrustLevel` mirrors and is compatible with the S11.1
`PublisherTrustLevel` lattice.

```text
RepoTrustLevel =
  AIOS_ROOT
| VERIFIED_UPSTREAM
| COMMUNITY_REVIEWED
| UNTRUSTED_COMMUNITY
| QUARANTINED
```

Unknown values are rejected by the repo-trust loader.

| Trust level           | Example                                                                  | Catalog                | System mutation                         |
| --------------------- | ------------------------------------------------------------------------ | ---------------------- | --------------------------------------- |
| `AIOS_ROOT`           | AIOS system packages, invariant bundles, recovery packages.              | Yes                    | Allowed under policy.                   |
| `VERIFIED_UPSTREAM`   | Official distro repos, verified Flathub publishers, signed vendor repos. | Yes                    | Allowed with approval.                  |
| `COMMUNITY_REVIEWED`  | Popular community packages with reproducible builds and clean history.   | Yes                    | Approval + shadow install required.     |
| `UNTRUSTED_COMMUNITY` | AUR-style recipes, arbitrary GitHub releases, unsigned AppImages.        | Observe/convert only   | Blocked without high-friction approval. |
| `QUARANTINED`         | Known malicious, capability-lie, revoked, abandoned, or compromised.     | Quarantine record only | Hard-denied.                            |

Firewall rules:

- Untrusted packages can be observed and translated, never blindly installed.
- System mutation requires high trust **plus** human approval.
- Repo scripts never receive direct root (INV-029).
- Every newly added repository emits an evidence-backed onboarding event before
  any package from it may be installed.
- A `QUARANTINED` source is hard-denied by the Policy Kernel and cannot be
  un-quarantined by an AI subject.

## 7. Package Rosetta pipeline

`PackageRosetta` is the compiler that converts foreign package semantics into
typed AIOS objects. It is the named "package solver translation stage." It never
executes foreign code on the host — it parses, observes (in the lab), and
translates.

```text
foreign package
  -> metadata parser           (control/spec/PKGBUILD/manifest/flake/OCI config)
  -> script observer           (preinst/postinst/scriptlet/PKGBUILD functions)
  -> dependency graph resolver (with per-dependency trust scoring)
  -> capability extractor      (files/network/GPU/devices/dbus/portals/secrets)
  -> sandbox profile proposal  (S3.2 SandboxProfile)
  -> install/verify/rollback plan
  -> install risk diff
  -> human approval (S5.3 EXACT_ACTION binding)
  -> AIOSAppObject (S12.1) handed to S17 capsule materialization
```

Inputs the Rosetta parser must understand:

- package metadata (`control`, RPM spec, `PKGBUILD`, `snapcraft.yaml`, Flatpak
  manifest, AppImage desktop metadata, Nix flake, OCI config)
- maintainer scripts (`preinst`, `postinst`, `prerm`, `postrm`, RPM scriptlets,
  AUR build functions)
- systemd units, timers, sockets, udev rules, dbus services, desktop files
- shared-library dependencies and ABI assumptions
- requested network endpoints
- filesystem write paths
- kernel module / DKMS / firmware needs (handed to S19, never executed here)
- SELinux/AppArmor profile hints where present

Outputs:

- AIOS package manifest and capability list
- `NetworkOutboundManifest` (S8.1 vocabulary)
- proposed `SandboxProfile` (S3.2)
- rollback plan (the everything-rollback set, §11)
- compatibility-score seed
- evidence requirements

## 8. Script Decompiler

Foreign install scripts are the single largest intake risk. The `ScriptDecompiler`
translates each observed script action into a typed AIOS action, or refuses.

```text
observed foreign action:
  mkdir /opt/foo
  cp service /etc/systemd/system/foo.service
  systemctl enable foo

AIOS typed translation:
  filesystem.create_dir(scope=app_private)
  service.install_unit(unit_hash=...)
  service.enable(unit_id=foo.service, approval=required)
```

Closed translation outcome enum:

```text
ScriptTranslationOutcome =
  TRANSLATED_FULLY
| TRANSLATED_WITH_APPROVAL
| TRANSLATED_PARTIAL_NEEDS_REVIEW
| UNTRANSLATABLE_ROUTE_TO_VM
| UNTRANSLATABLE_BLOCKED
```

Unknown values are rejected by the decompiler.

Rules:

- A script action that cannot be translated to a typed action is never executed.
  It is either routed to a VM workload (`UNTRANSLATABLE_ROUTE_TO_VM`) or blocked
  (`UNTRANSLATABLE_BLOCKED`), with the exact untranslatable action recorded.
- No translated action runs as root unless it maps to a typed action that the
  Policy Kernel independently authorizes for a non-AI subject.
- Mutations outside the app-private scope (`/usr`, `/etc`, broad `$HOME`,
  `/boot`, kernel module load) are flagged in the install risk diff and require
  explicit human approval; AI subjects can never approve them (INV-013).

## 9. Universal App Lab and Shadow Install

The Universal App Lab extends **S12.1 Phase A** (observe) from app-compatibility
into a general package-intake mechanism. Unknown or untrusted software is observed
in maximum-restriction isolation before any install.

`ShadowInstall` is the dry-install mechanism. It performs the install into an
overlay/ephemeral namespace, observes the resulting mutations, and never touches
the live host.

```text
ShadowInstall:
  foreign package
    -> overlay root / ephemeral mount namespace (off the active /aios root)
    -> run maintainer scripts ONLY inside the sandbox (never on host, never root)
    -> observe filesystem reads/writes, DNS, outbound sockets, GPU init,
       dbus, portals, device opens, process launches
    -> redact payloads; record only structural behavior
    -> translate observed mutations into typed AIOS actions (Script Decompiler)
    -> verify no forbidden writes occurred
    -> PROMOTE (hand AIOSAppObject to S17) or DISCARD (tear down overlay)
```

Closed shadow-install result enum:

```text
ShadowInstallResult =
  CLEAN_PROMOTABLE
| PROMOTABLE_WITH_APPROVAL
| FORBIDDEN_WRITE_DETECTED
| BREAKOUT_ATTEMPT_QUARANTINED
| INCONCLUSIVE_ROUTE_TO_VM
```

Unknown values are rejected by the lab result loader.

Lab guarantees:

- Maintainer scripts execute only inside the lab sandbox, never on the host and
  never as root (INV-029).
- A breakout attempt (`BREAKOUT_ATTEMPT_QUARANTINED`) moves the source toward
  `QUARANTINED` trust and emits FOREVER-retention evidence.
- The full dependency graph is known before promotion, so rollback is cheap.
- Discard is the default on any forbidden write; promotion is the exception that
  requires evidence.

## 10. Install Risk Diff

Before approval, S21 renders a structured `InstallRiskDiff` produced from typed
facts gathered in the lab — never from natural-language guesswork.

```text
InstallRiskDiff for <app> (runtime: FLATPAK_STYLE):

This install requests:
  + network: api.vendor.example:443
  + filesystem write: /aios/groups/work/apps/<app>/
  + systemd service: vendor-sync.service
  + dbus service: org.vendor.Sync
  + GPU class: GPU_FULL_3D
  + background autostart

Denied unless operator approves (AI may never approve these):
  ! broad home read
  ! kernel module build
  ! postinstall script wants /usr/bin mutation

Compatibility: 82%   Install risk: MEDIUM   Supply-chain risk: LOW
Rollback: full (files + deps + units + dbus + config)
Evidence: shadow_install_receipt evr_...
```

The diff is the operator's decision surface. Each `+` line is a requested
capability already proven in the lab; each `!` line is a high-risk mutation
gated behind explicit human approval bound to the exact action (S5.3
`EXACT_ACTION`). The renderer never invents a `+` or `!` line that lacks a
backing observed fact.

## 11. Package Solver

The Package Solver is the holistic §6 universal solver instantiated for packages.
It is the solver named "Capsule Solver / package solver" in the holistic
architecture and it lives here. It consumes the catalogued candidates, the active
`SecurityProfile`, the S18 `KernelCapabilityMatrix`, the S3.2 floor, and per-source
trust, and emits one `SmartInstallDecision`.

```text
SmartInstallDecision =
  NATIVE_CONVERTED
| NATIVE_ISOLATED
| FLATPAK_STYLE
| NIX_ENV
| DISTROBOX_CONTAINER
| APPIMAGE_EXTRACTED
| OCI_CONTAINER
| VM_FALLBACK
| WASI
| BLOCKED_WITH_REASON
```

Unknown values are rejected by the solver-output loader.

| Decision              | When chosen                                                                                              |
| --------------------- | -------------------------------------------------------------------------------------------------------- |
| `NATIVE_CONVERTED`    | Clean Linux app, no system mutation beyond declared app-private paths.                                   |
| `NATIVE_ISOLATED`     | Needs native ABI but runs under an AIOS sandbox profile.                                                 |
| `FLATPAK_STYLE`       | GUI app with portal-compatible permissions.                                                              |
| `NIX_ENV`             | Reproducible CLI/dev tool or dependency-heavy app.                                                       |
| `DISTROBOX_CONTAINER` | App strongly assumes a specific distro userspace.                                                        |
| `APPIMAGE_EXTRACTED`  | Portable binary with unknown manifest; extract and sandbox.                                              |
| `OCI_CONTAINER`       | Server/service packaged as a container image.                                                            |
| `VM_FALLBACK`         | Needs privileged services, fragile ABI, kernel driver, anti-cheat, hard DRM, or unsupported OS behavior. |
| `WASI`                | Portable component compilable to a WASI sandbox; strongest isolation, no host ABI.                       |
| `BLOCKED_WITH_REASON` | Unsafe, unverifiable, legally impossible, or requires host mutation outside policy.                      |

The solver follows the universal pattern exactly: inspect signed state → generate
candidates → score → risk diff → policy → test off the active system (shadow
install / lab) → promote with evidence → rollback or block with reason. It does
not invent a parallel flow.

### One app, many backends

When the same app is available from multiple sources, the solver scores every
backend and recommends one path:

```text
recommended = argmax(
    security_score
  + compatibility_score
  + update_reliability
  + rollback_quality
  - runtime_cost
  + hardware_fit
  + user_preference
)
```

Backends compared include distro native, Flatpak, Snap, AppImage, Nix, source
build, OCI image, the Windows version via S17 Proton, and the Android version via
S17 Waydroid/VM. The recommendation, its score, and the rejected alternatives are
all recorded in the passport and shown in the risk diff. This is stronger than
"install the first package found."

### Dependency quarantine

Dependencies are not automatically innocent. The solver scores each dependency on
source-repo trust, publisher trust, known CVEs, maintainer-script behavior,
transitive network capability, native/FFI/setuid surface, and kernel-module or
firmware requests. A suspicious dependency may be replaced by a safer provider,
isolated in a container, pinned to an older safe version, blocked, or routed to a
VM. A dependency from a `QUARANTINED` source blocks the whole install.

## 12. Rollback contract

No package install is complete unless rollback is known. The everything-rollback
set covers:

- package files
- dependency graph snapshot
- generated configuration
- systemd units / timers / sockets
- dbus policy
- udev rules
- SELinux modules / labels
- firewall / network grants
- desktop entries and file associations
- user-state migration

Rollback never restores a package to a known-vulnerable version (inherited from
S12.2). The rollback plan id is recorded in the passport before promotion; a
promotion attempted without a rollback plan is denied by the Policy Kernel.

## 13. Security profile gates

| Profile          | Intake / install rule                                                                                                                               |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `DEV_RELAXED`    | All sources catalogued; untrusted sources installable with warning + shadow install + rollback.                                                     |
| `SECURE_DEFAULT` | Verified/reviewed sources preferred; `UNTRUSTED_COMMUNITY` requires shadow install + human approval; `QUARANTINED` denied.                          |
| `STIG_ALIGNED`   | Signed `AIOS_ROOT`/`VERIFIED_UPSTREAM` only unless an expiring, owner-bound exception exists; SBOM + provenance required; no untranslatable script. |
| `AIRGAP_HIGH`    | Signed local mirror only; no live `GITHUB_RELEASE`/vendor fetch; SBOM + provenance mandatory; VM fallback only via local images.                    |

`FIPS_STRICT` is an overlay (S16.5): when active, only crypto-validated package
verification paths are accepted, and a package whose signature verification cannot
run through a validated module is blocked.

Hard denies (Policy Kernel, all profiles):

- No maintainer or foreign install script runs as root or on the live host
  (INV-029).
- No AI subject may approve a package install, a repo trust change, a quarantine
  release, an `UNTRANSLATABLE_*` override, or a security-profile-weakening
  exception.
- No package is promoted without a rollback plan.
- No `QUARANTINED` source is installable.
- No passport asserts a capability that was not observed in the lab.
- No install bypasses the S3.2 sandbox floor.

## 14. Evidence records

S21 adds these record types:

```text
PACKAGE_INTAKE_OBSERVED
ROSETTA_TRANSLATION_COMPLETED
SHADOW_INSTALL_RESULT
INSTALL_RISK_DIFF_RENDERED
REPO_TRUST_EVALUATED
PACKAGE_PASSPORT_ISSUED
PACKAGE_PASSPORT_UPDATED
DEPENDENCY_QUARANTINED
SCRIPT_TRANSLATION_BLOCKED
PACKAGE_INSTALL_PROMOTED
PACKAGE_INSTALL_ROLLED_BACK
PACKAGE_SOURCE_QUARANTINED
```

Minimum fields for `PACKAGE_PASSPORT_ISSUED`:

```text
passport_id
app_id
source_kind
repo_trust_level
selected_runtime
isolation_floor
install_risk
runtime_risk
supply_chain_risk
sbom_ref
provenance_grade
rollback_plan_id
shadow_install_receipt
policy_decision_id
security_profile
evidence_receipt_id
```

Minimum fields for `SCRIPT_TRANSLATION_BLOCKED`:

```text
app_id
source_kind
script_phase            # preinst|postinst|prerm|postrm|scriptlet|build
observed_action_redacted
translation_outcome     # ScriptTranslationOutcome
forbidden_target
security_profile
evidence_receipt_id
```

## 15. Non-goals

- Do not promise that every package, format, or foreign app will run natively.
- Do not run maintainer scripts, `PKGBUILD` functions, RPM scriptlets, or vendor
  installers as root or directly on the live host.
- Do not treat catalog inclusion as trust; cataloging is not endorsement.
- Do not let an untrusted or quarantined source install without observation.
- Do not let an AI subject approve installs, trust changes, or quarantine
  releases.
- Do not weaken the S3.2 sandbox floor or any `SecurityProfile` to make a package
  installable.
- Do not duplicate the S17 capsule runtime; S21 produces inputs, S17 runs them.
- Do not duplicate S19 driver handling; kernel-module/firmware needs are handed to
  S19, never satisfied here.

## 16. Acceptance criteria

S21 is `REAL` only when:

1. The `PackageSource` intake loader classifies every supported format and rejects
   unidentifiable artifacts into the lab rather than installing them.
2. The `RepoTrustLevel` firewall is enforced: untrusted sources catalog and
   observe but cannot install without high-friction human approval, and
   `QUARANTINED` sources are hard-denied.
3. `PackageRosetta` parses metadata and scripts for at least the `DEB_PACKAGE`,
   `RPM_PACKAGE`, `ARCH_PKGBUILD`, `FLATPAK_BUNDLE`, and `OCI_IMAGE` sources and
   emits a typed capability list, sandbox profile, and rollback plan.
4. The `ScriptDecompiler` translates observed script actions into typed actions
   and blocks or VM-routes any untranslatable action, recording the exact action.
5. A maintainer script never executes on the live host or as root; it executes
   only inside the Universal App Lab / Shadow Install sandbox (INV-029).
6. `ShadowInstall` performs an overlay/ephemeral dry install, detects forbidden
   writes and breakout attempts, and promotes only on a clean or approved result.
7. `InstallRiskDiff` is rendered from observed typed facts, with every `+`/`!`
   line backed by an evidence receipt.
8. The Package Solver emits a `SmartInstallDecision`, scores multiple backends for
   the same app, and records the recommendation plus rejected alternatives.
9. A `PackagePassport` is issued for every catalogued package and updated on
   every install, launch, and policy decision, with no asserted capability that
   was not observed.
10. The everything-rollback set is captured before promotion and never restores a
    known-vulnerable version; promotion without a rollback plan is denied.
11. Security profile gates apply: `STIG_ALIGNED`/`AIRGAP_HIGH` reject unsigned and
    provenance-less packages and untranslatable scripts.
12. No AI subject can approve an install, change repo trust, release a quarantine,
    or weaken a profile.

## 17. See also

- [S12.1 App Runtime Model](../../002.AI-OS.NET--SPECREV.2/L6_Apps_Packages_Compatibility/01_app_runtime_model.md)
- [S12.2 Package Model](../../002.AI-OS.NET--SPECREV.2/L6_Apps_Packages_Compatibility/02_package_model.md)
- [S3.2 Sandbox Composition](../../002.AI-OS.NET--SPECREV.2/L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [S11.1 Repository Model and Trust Roots](../../002.AI-OS.NET--SPECREV.2/L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [S11.3 External Integrations Bridge](../../002.AI-OS.NET--SPECREV.2/L10_Distribution_Ecosystem_Marketplace/03_external_integrations.md)
- [S2.3 Policy Kernel](../../002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S5.3 Approval Mechanics](../../002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/04_approval_mechanics.md)
- [S17 App Capsule Runtime](../S17_App_Capsule_Runtime/00_overview.md)
- [S16 Security Hardening and Compliance](../S16_Security_Hardening_Compliance/00_overview.md)
- [S19 Driver and Firmware Capsule Plane](../S19_Driver_Firmware_Capsule_Plane/00_overview.md)
- [Rev.3 Holistic Specification](../00_REV3_HOLISTIC_SPEC.md)
- [Rev.3 Planning Notes](../00_PLANNING_NOTES.md)
