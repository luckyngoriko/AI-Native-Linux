# External Integrations — Bridges to Flathub, OCI Registries, Distro Repos (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists, structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| Phase tag      | S11.3                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Layer          | L10 Distribution, Ecosystem, Marketplace                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Schema package | `aios.bridge.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| Consumes       | L0 INV-002 (AI proposes never executes), INV-008 (default-deny policy), INV-013 (AI cannot perform system admin), INV-014 (no proof no completion), INV-017 (sandbox floor constitutional); S11.1 Repository Model (`PublisherTrustLevel`, `RepositoryKind = EXTERNAL_BRIDGE`, `PackageManifest`, install pipeline FSM, `MirrorSemantic`, `TakedownReason`, capability-lie audit); S12.1 App Runtime Model (`EcosystemRuntime` enum, `RUNTIME_FLATPAK` / `RUNTIME_DISTROBOX` / `RUNTIME_LINUX_NATIVE`, `RecipeTrustClass`, four-phase setup, community recipe registry, honesty principle); S5.3 Approval Mechanics (`EXACT_ACTION` binding, `HUMAN_USER` approver); S3.2 Sandbox Composition (`SandboxProfile`, `ISOLATED_SANDBOX`, runtime safety floor); S0.1 Action Envelope (typed actions, FSM); S3.1 Evidence Log (`RecordType` vocabulary, retention classes); S8.1 Network Policy (`NetworkOutboundManifest`); S2.3 Policy Kernel (`EvaluatePolicy`); S9.1 Recovery Boundary (first-boot system bootstrap path) |
| Produces       | typed `BridgeSource` / `BridgeOperationKind` / `UpstreamSignatureKind` / `BridgeAdmissionResult` enums; the bridge admission pipeline that re-packages upstream content under an AIOS bridge-signing key; the per-source policy table (Flathub / OCI / distro); the metadata-only and recipe-only import disciplines; per-bridge sandbox and rate limits; the deceptive-trust-class rejection rule; twelve evidence record types queued for S3.1; bounded-cardinality telemetry contract; three worked examples                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |

## §1 Purpose

External ecosystems are where most of the world's open-source software actually lives. Flathub holds the curated Linux desktop catalogue; OCI registries (Docker Hub, GitHub Container Registry, Quay) hold the container ecosystem; distro repositories (Debian, Ubuntu, Fedora, Arch) hold the system-bootstrap substrate that AIOS itself sits on top of. AIOS cannot pretend to be self-sufficient; it must reach into these ecosystems for the operator's benefit, and it must do so without surrendering the trust roots defined by S11.1.

This sub-spec defines that reach. It is the **bridge contract**: the rules by which an upstream Flathub package, an OCI registry image, or a distro `.deb`/`.rpm` package becomes an AIOS-shaped object that can flow through the S11.1 install pipeline, be audited by the S11.1 first-run capability lie audit, and (where applicable) feed the S12.1 community recipe registry.

The contract is built on four constitutional rules that are non-negotiable:

1. **Bridges never reach `AIOS_ROOT` or `VERIFIED` trust.** Upstream signatures are trusted only insofar as the bridge admits them; AIOS does not promote upstream reputation into AIOS-side trust class. `COMMUNITY` is the ceiling.
2. **The bridge fetcher itself runs sandboxed.** No bridge process holds capabilities outside the `ISOLATED_SANDBOX` floor (S3.2); a bridge that compromises its own infrastructure cannot reach the AIOS host through capability escalation.
3. **Local Phase A pre-flight always wins.** Imported metadata, imported recipes, and upstream signatures are all metadata-only — the local Phase A observation (S12.1 §4.1) and the S11.1 first-run capability lie audit decide what the package actually does. Imported reputation is never substituted for local audit.
4. **Deceptive trust claims are rejected forever.** A bridged package whose synthesised manifest claims any AIOS-side trust class higher than `COMMUNITY`, or whose imported metadata claims an AIOS_ROOT/AIOS_VERIFIED affiliation it does not have, is rejected at admission and the rejection is FOREVER evidence.

This contract is the deepening of S11.1 §14 (which named the bridge boundary) and the operationalisation of S12.1 §6.5 (which named imported recipes). It does not redefine the S11.1 trust chain; it does not redefine the S12.1 EcosystemRuntime model; it cites both and adds the per-bridge mechanics that turn an upstream world into an AIOS-shaped world.

## §2 Scope

This spec **defines**:

1. The closed `BridgeSource` enum with five values (`FLATHUB`, `OCI_REGISTRY`, `DISTRO_DEB`, `DISTRO_RPM`, `OTHER_BRIDGED`).
2. The closed `BridgeOperationKind` enum with four values (`PACKAGE_FETCH`, `PACKAGE_REPACKAGE`, `METADATA_IMPORT`, `RECIPE_IMPORT`).
3. The closed `UpstreamSignatureKind` enum with eight values covering GPG, Flatpak OSTree-bound GPG, OCI cosign, distro RPM GPG, distro DEB GPG, Debian-format detached signatures, signed tarballs, and the explicit unsigned-rejection terminal.
4. The closed `BridgeAdmissionResult` enum with five outcomes (`ADMITTED_COMMUNITY`, `ADMITTED_WITH_OPERATOR_CONSENT`, `DEFERRED_NEEDS_REVIEW`, `REJECTED_UNSIGNED`, `REJECTED_DECEPTIVE`).
5. The bridge architecture: a bridge is itself an AIOS package (per S11.1 §14.1) running under an `ISOLATED_SANDBOX` profile; its operator identity is a `_system:service:bridge-<source>` service subject signed by the AIOS bridge publisher (a `VERIFIED` publisher whose role is bridging).
6. The bridge admission pipeline: fetch → upstream-signature verify → repackage-with-AIOS-bridge-key → S11.1 install-pipeline entry point. Each step has a closed failure outcome and FOREVER evidence on failure.
7. The metadata-only and recipe-only import disciplines: separate from package import; manifests, ratings, descriptions, recipes can be imported without bringing the upstream package itself; imported metadata never auto-grants higher trust.
8. The per-source policy table that closes which `BridgeOperationKind` values each `BridgeSource` admits; e.g. Flathub admits `PACKAGE_FETCH` + `METADATA_IMPORT` + `RECIPE_IMPORT`, OCI admits only `METADATA_IMPORT` (no auto-package-import; operator must explicitly fetch), distro repos admit only `PACKAGE_FETCH` for first-boot bootstrap (not user-facing apps).
9. The rate-limit contract: per-bridge per-upstream-source budgets with deferral on excess.
10. The reputation tracking and auto-blacklist discipline for bridges that misbehave (bridge-as-malware-distributor mitigation).
11. The deceptive-trust-claim rejection: any bridged package whose synthesised manifest or imported metadata claims `AIOS_ROOT` or `VERIFIED` trust is rejected with `REJECTED_DECEPTIVE` and FOREVER evidence; the rejection is permanent.
12. The bridge-as-AIOS-publisher discipline: the AIOS bridge-signing key is itself a publisher root (per S11.1 §4.2) at trust level `AIOS_ROOT` — i.e. the bridge infrastructure is operated by AIOS-root — but it MUST NOT issue manifests claiming `AIOS_ROOT` or `VERIFIED` trust on bridged packages; the bridge publisher's authority is bounded to `COMMUNITY`-trust admissions only.
13. Adversarial robustness: upstream signature compromise; supply-chain attack on bridged packages; deceptive metadata; rate-limit evasion; bridge-as-malware-distributor; bridged package claiming AIOS_VERIFIED.
14. Twelve evidence record types queued for S3.1.
15. Bounded-cardinality telemetry contract.
16. Three worked examples (Flathub package admitted as COMMUNITY; OCI metadata-only import; distro repo bootstrap during first-boot).

This spec **does not** define:

- The S11.1 trust chain itself (cited; not redefined).
- The S12.1 `EcosystemRuntime` enum or the per-runtime sandbox profile shape (cited; the bridge selects an `EcosystemRuntime` from the closed S12.1 vocabulary at repackage time).
- The Flathub OSTree wire protocol, the OCI registry distribution-spec wire format, or the apt/dnf/pacman repository wire formats — these are upstream concerns; the bridge speaks each upstream's transport without re-defining it.
- The first-boot system bootstrap orchestration (S9.1 owns recovery boundary; S1.x owns boot flow). This contract names only the bridge's role in that bootstrap.
- Translation of upstream metadata into the L5 capability catalog (S12.1 §3.3 `ManifestTranslationStrategy` enum already covers `FLATPAK_MANIFEST_JSON` etc.; this contract cites and reuses).
- Marketplace UX for displaying bridge-sourced apps (`02_marketplace.md` — `SHELL`).
- HSM-backed AIOS bridge-signing keys (deferred per S11.1 §21).
- Cross-host bridge cache federation (deferred).

## §3 Vocabulary (closed enums)

All vocabularies in this section are closed. Adding a value is a versioned spec change. Bridge fetchers, repackagers, manifest validators, and the install pipeline MUST reject values outside the enum at parse time. None of these enums admits an `OPEN` or `OTHER_FREEFORM` value beyond the explicit `OTHER_BRIDGED` slot; the intent is to make bridge semantics fully mechanical.

### §3.1 `BridgeSource`

Closed enum, five values. Each value identifies an upstream ecosystem and binds the per-source policy table in §6.

| Value           | Semantics                                                                                                                       | Per-source policy summary                                                                                  |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `FLATHUB`       | The Flathub Flatpak distribution. Curated, OSTree-bound GPG signatures, manifests in well-known shape.                          | `PACKAGE_FETCH` + `METADATA_IMPORT` + `RECIPE_IMPORT`; admits at `COMMUNITY` trust.                        |
| `OCI_REGISTRY`  | OCI-spec registries (Docker Hub, GHCR, Quay, registry.k8s.io, etc.). Supports cosign signatures; widely heterogeneous quality.  | `METADATA_IMPORT` only; **no auto-package-import**; operator must explicitly fetch via a typed action.     |
| `DISTRO_DEB`    | Debian / Ubuntu / Mint / derivative repositories. Signed `Release` files; per-package GPG via `Release.gpg` and `InRelease`.    | `PACKAGE_FETCH` for **first-boot system bootstrap only**; never for user-facing apps.                      |
| `DISTRO_RPM`    | Fedora / RHEL / openSUSE / derivative repositories. Per-package GPG signatures.                                                 | Same as `DISTRO_DEB` — first-boot bootstrap only; never for user-facing apps.                              |
| `OTHER_BRIDGED` | A future bridge added by versioned spec change. Reserved slot; this contract carries no `OTHER_BRIDGED` admission rules itself. | Reserved; admissions under `OTHER_BRIDGED` require explicit per-bridge sub-spec at the time of activation. |

The enum is closed. A bridge reporting any other source string at admission is rejected at the bridge-pipeline parse step; a manifest carrying any other `bridge_source` field is rejected at S11.1 step 6 (manifest field validation) with `MANIFEST_FORGED`.

### §3.2 `BridgeOperationKind`

Closed enum, four values. Each value identifies the kind of operation a bridge is performing, which gates which sandbox profile floor and which evidence records apply.

| Value               | Semantics                                                                                                                                                                                                                                                                                                                                            |
| ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `PACKAGE_FETCH`     | The bridge fetches an upstream package payload (Flatpak bundle, OCI image layers, distro `.deb`/`.rpm` blob) into a content-addressed staging area. Produces no AIOS manifest by itself.                                                                                                                                                             |
| `PACKAGE_REPACKAGE` | The bridge takes a previously-fetched payload, validates upstream signature, and synthesises an AIOS `PackageManifest` per S11.1 §5; signs it with the AIOS bridge-signing key. Output is a regular AIOS package that flows through the S11.1 install pipeline.                                                                                      |
| `METADATA_IMPORT`   | The bridge imports manifest metadata, ratings, descriptions, screenshots — **without** importing the package payload. Produces a metadata object stored in the S12.1 community recipe registry (or an L7-marketplace metadata cache). The package itself is fetched only on explicit operator action.                                                |
| `RECIPE_IMPORT`     | The bridge imports an AIOS recipe per S12.1 §6.5 from upstream community knowledge (ProtonDB, Flathub manifests, Snapcraft store, AUR PKGBUILDs). Produces an `AppRecipe` (S12.1 §6.1) of `RecipeTrustClass = RECIPE_IMPORTED` with full `upstream_attribution`. The recipe is metadata-only; local Phase A and Phase C audits remain authoritative. |

A `PACKAGE_REPACKAGE` operation requires that a `PACKAGE_FETCH` for the same upstream artifact succeeded earlier in the same bridge run (or a previous run) and that the fetched payload is still in the staging area with its upstream signature still verifiable. `PACKAGE_REPACKAGE` without a corresponding `PACKAGE_FETCH` is rejected at the pipeline parse step.

### §3.3 `UpstreamSignatureKind`

Closed enum, eight values. Each value identifies the cryptographic shape of the upstream signature the bridge must verify before it may proceed to `PACKAGE_REPACKAGE`. The enum is closed; an unknown shape is treated as `UNSIGNED_REJECTED`.

| Value                     | Source / shape                                                                                                                                                      |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GPG`                     | Generic GPG-detached signature over the artifact (e.g. signed-tarball releases on a vendor's website that the bridge accepts as upstream input).                    |
| `FLATPAK_OSTREE_GPG`      | Flatpak-specific OSTree-bound GPG signature over the OSTree commit; verified by the Flatpak/OSTree library in the bridge sandbox.                                   |
| `OCI_COSIGN`              | OCI cosign signature (Sigstore) attached to the OCI image manifest; verified per the cosign verification protocol.                                                  |
| `DISTRO_RPM_GPG`          | RPM per-package GPG signature embedded in the RPM header, verified against the repository's `repomd.xml.asc` chain.                                                 |
| `DISTRO_DEB_GPG`          | Detached GPG over the `Release` / `InRelease` file in a Debian repository, with per-package `Packages` chain via SHA hash to the `Release` file.                    |
| `DISTRO_DEB_DEBIAN_FORMS` | Debian-format detached signature on a source-package `.dsc` file — the multi-file format used for source packages and some control artefacts.                       |
| `SIGNED_TAR`              | Signed tarball with a sidecar signature file (e.g. `*.tar.xz` + `*.tar.xz.sig`) using a known vendor key from the bridge's pinned-key catalogue.                    |
| `UNSIGNED_REJECTED`       | The terminal value for any artifact that does not present a recognised upstream signature. Bridge MUST emit `BRIDGE_UPSTREAM_SIGNATURE_FAILED` (FOREVER) and abort. |

The enum is closed. A bridge attempting to repackage a payload whose `UpstreamSignatureKind` is `UNSIGNED_REJECTED` is rejected at the verify step (§5.2) with `BridgeAdmissionResult = REJECTED_UNSIGNED`. The bridge MUST NOT synthesise an AIOS manifest for an unsigned upstream artifact — there is no operator-override path; unsigned upstream is constitutionally inadmissible.

### §3.4 `BridgeAdmissionResult`

Closed enum, five outcomes. Every bridge admission attempt produces exactly one outcome, which is recorded in the corresponding evidence record.

| Value                            | Semantics                                                                                                                                                                                                                                                                                                                                    |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ADMITTED_COMMUNITY`             | Upstream signature verified; AIOS bridge manifest synthesised; the package flows into the standard S11.1 install pipeline at `RepositoryKind = EXTERNAL_BRIDGE`, `PublisherTrustLevel = COMMUNITY`. The default happy path for `PACKAGE_REPACKAGE`.                                                                                          |
| `ADMITTED_WITH_OPERATOR_CONSENT` | Upstream signature verified but the operator was prompted with an additional consent dialog (e.g. macOS-VM legal-grey-area note from S12.1; first-time use of a freshly-imported bridge; reactivation of a previously rate-limited bridge). Operator granted consent via S5.3 `EXACT_ACTION` binding before admission.                       |
| `DEFERRED_NEEDS_REVIEW`          | Upstream signature verified but a structural anomaly was detected (e.g. upstream metadata claims an AIOS_ROOT/AIOS_VERIFIED affiliation that the bridge cannot confirm; manifest delta vs prior version exceeds a magnitude threshold). The admission is deferred to operator review; the package does not enter the install pipeline.       |
| `REJECTED_UNSIGNED`              | Upstream signature is `UNSIGNED_REJECTED` or signature verification failed. The package is rejected; FOREVER evidence; no operator override.                                                                                                                                                                                                 |
| `REJECTED_DECEPTIVE`             | The bridged package's synthesised manifest or imported metadata contains a deceptive trust-class claim (e.g. `publisher_trust = VERIFIED` on a `EXTERNAL_BRIDGE` repository origin; imported metadata claims AIOS_VERIFIED affiliation that cannot be substantiated). The package is rejected; FOREVER evidence; the rejection is permanent. |

`REJECTED_DECEPTIVE` is permanent. A bridge that produces a `REJECTED_DECEPTIVE` outcome on a given upstream artifact will continue to produce `REJECTED_DECEPTIVE` on the same artifact unless the upstream artifact itself is changed (different content hash). The rejection is keyed on the upstream content hash, not on the bridge run id; re-running the bridge on the same bytes does not re-test admission.

## §4 Bridge architecture

A bridge is itself an AIOS package. Per S11.1 §14.1, the bridge package has `PackageKind = APP` (or a future dedicated bridge kind) and is published by the AIOS bridge publisher.

### §4.1 The AIOS bridge publisher

Bridges are operated by AIOS-root through a dedicated publisher root. Mechanically:

1. **Identity.** The AIOS bridge publisher is a publisher root per S11.1 §4.2 with `publisher_root_id = pub:aios-bridge` and `PublisherTrustLevel = AIOS_ROOT`. Its public key is recorded in the AIOS-root-signed publisher catalog (`pubcat_<hex>`) and is itself signed by the AIOS root key per the standard three-tier chain.
2. **Bounded authority.** Despite holding `AIOS_ROOT` trust, the bridge publisher is **constitutionally bounded** to issue only manifests whose `originating_repository = EXTERNAL_BRIDGE` and whose `publisher_trust = COMMUNITY`. A manifest from `pub:aios-bridge` claiming any other origin or any other trust class is rejected at S11.1 step 6 (manifest field validation) with `MANIFEST_FORGED` and `BridgeAdmissionResult = REJECTED_DECEPTIVE`.
3. **Per-source signing keys.** The AIOS bridge publisher issues one or more per-source package signing keys (per S11.1 §4.3): `pks:aios-bridge:flathub`, `pks:aios-bridge:oci`, `pks:aios-bridge:distro-deb`, `pks:aios-bridge:distro-rpm`. Rotation follows S11.1 §11 publisher key rotation discipline.
4. **The AIOS bridge-signing key is a publisher root** — i.e. the bridge is operated by AIOS-root — but it is NOT a back door to AIOS_ROOT-trust admission. The constitutional bound in (2) above is enforced at the manifest field validation step regardless of the signing key's identity.

The reason for this asymmetry is: the AIOS bridge publisher needs `AIOS_ROOT` trust to be a first-class participant in the publisher catalog (so bridged packages have a real, recoverable, deplatformable publisher root); but it must not be allowed to admit upstream packages at high trust because the upstream content has not been audited at AIOS-side. The constitutional bound resolves the asymmetry mechanically.

### §4.2 The bridge service subject

Each bridge runs as a system service subject:

- `_system:service:bridge-flathub`
- `_system:service:bridge-oci`
- `_system:service:bridge-distro-deb`
- `_system:service:bridge-distro-rpm`

Per S0.1 / L4 identity model, the service subject has `is_ai = false`, `is_recovery_mode = false`, and is bound to a system-scope identity with `system_admin = false`. The service subject can fetch from upstream (per §4.3 sandbox profile), can compute hashes and verify upstream signatures, and can submit `bridge.repackage` typed actions to the AIOS install pipeline. It cannot install packages directly; the install proceeds under the standard S11.1 pipeline with the operator (or a system-bootstrap subject under recovery for distro repos) as the executing subject.

### §4.3 The bridge sandbox profile

Every bridge runs under an `ISOLATED_SANDBOX` profile per S3.2 §5 / S12.1 §4.1 with the following bridge-specific tightening:

```yaml
filesystem:
  root_mode: NO_ACCESS
  allow_write:
    - /aios/system/runtime/bridge/<bridge_source>/<bridge_run_id>/staging
    - /aios/system/runtime/bridge/<bridge_source>/<bridge_run_id>/scratch
  allow_read:
    - /aios/system/runtime/bridge/<bridge_source>/keys/upstream-pinned-public-keys
  tmpfs_for_tmp: true
  home_isolation: true
network:
  mode: EXPLICIT_ALLOWLIST
  allowlist:
    - HOST_FQDN # the upstream's authoritative host(s); per-source list
  outbound_byte_budget: 4_294_967_296 # 4 GiB per bridge run; configurable
process:
  seccomp_profile_id: aios.bridge-narrow
  no_new_privileges: true
  drop_all_capabilities: true
  allow_user_namespace: false
  allow_ptrace: false
  max_processes: 30
resources:
  cpu_weight: 100
  memory_max_bytes: 1_073_741_824 # 1 GiB
  pids_max: 100
secrets:
  mode: BROKER_ONLY
  allowed_capabilities:
    - KEY_SIGN: vault://aios/bridge/<bridge_source>/signing
gpu_policy:
  gpu_capability_class: GPU_NONE
  deny_compute_pipeline: true
evidence:
  capture_stdout: STRUCTURED # bridge logs are structured; no free-form output
```

The sandbox floor is constitutional per INV-017. No bridge package, no upstream upgrade, no operator override can loosen it. A bridge that attempts to write outside its `allow_write` tree, or to reach a host not in the `allowlist`, or to invoke a capability outside its declared set is hard-denied at the sandbox enforcer and emits FOREVER `BRIDGE_DECEPTIVE_REJECTED` (with sub-reason `SANDBOX_BREAKOUT_ATTEMPTED`) plus the standard sandbox-violation evidence from L8 / S3.2.

The sandbox is **per-bridge-run**, not per-bridge-package. Each `bridge_run_id` (a fresh ULID-26 generated at run start) has its own staging directory; bridge runs cannot share filesystem state across runs except through the AIOS-managed pinned-public-keys catalogue (read-only) and the AIOS-managed staging garbage collector.

### §4.4 The pinned-public-keys catalogue

For each `BridgeSource`, the AIOS bridge infrastructure ships a pinned-public-keys catalogue:

- `flathub-pinned-keys.json` — the Flathub project's GPG public keys, signed by the AIOS bridge publisher.
- `oci-cosign-roots.json` — the Sigstore Fulcio root + key trust policy for OCI cosign verification, signed by the AIOS bridge publisher.
- `distro-deb-keys.json` — Debian, Ubuntu, Linux Mint archive keys, signed by the AIOS bridge publisher.
- `distro-rpm-keys.json` — Fedora, RHEL, openSUSE archive keys, signed by the AIOS bridge publisher.

Each catalogue is content-addressed, AIOS-bridge-publisher-signed, and read-only from the bridge's sandbox. Catalogue updates flow as `CAPABILITY_CATALOG_DELTA`-equivalent S11.1 packages (recovery-only per S11.1 §3.4 Conditional row; or hot-loadable in normal mode if AIOS root co-signs the delta).

A bridge that encounters an upstream artifact signed by a key not present in its pinned-public-keys catalogue treats it as `UNSIGNED_REJECTED` regardless of the upstream's claim that the key is "official". The pinned catalogue is the authoritative trust anchor for upstream signatures; bridges do not trust upstream key servers (e.g. `keys.openpgp.org`) directly because key servers can be MITM'd or compromised. The catalogue is updated only by AIOS-bridge-publisher-signed deltas.

## §5 The bridge admission pipeline

Strictly ordered, fail-closed. Every step has a closed failure outcome from `BridgeAdmissionResult`. Any step failing returns the pipeline to the failure outcome with FOREVER (or extended-60M) evidence.

```text
┌───────────────────────────────────────────────────────────────────────┐
│                      BRIDGE ADMISSION PIPELINE                        │
├───────────────────────────────────────────────────────────────────────┤
│  1. Operation kind dispatch    (PACKAGE_FETCH / PACKAGE_REPACKAGE /   │
│                                 METADATA_IMPORT / RECIPE_IMPORT)      │
│  2. Rate-limit check            (per-source budget; defer if over)    │
│  3. Fetch from upstream         (sandboxed; outbound to allowlist)    │
│  4. Upstream signature verify   (UpstreamSignatureKind dispatch)      │
│  5. Deceptive-claim check       (upstream metadata vs AIOS-side trust)│
│  6. Repackage with AIOS bridge  (only PACKAGE_REPACKAGE; synthesise   │
│     signing key                  manifest at COMMUNITY trust;          │
│                                  record bridge audit metadata)        │
│  7. Hand off to S11.1           (PACKAGE_REPACKAGE only; pipeline     │
│     install pipeline             enters at step 1 fetch — already     │
│                                  fetched; or re-stages content)       │
│  8. Emit admission evidence     (STANDARD_24M on success; FOREVER /   │
│                                  EXTENDED_60M on failure)             │
└───────────────────────────────────────────────────────────────────────┘
```

### §5.1 Step 1 — Operation kind dispatch

The bridge run begins with a typed action submission per S0.1:

```text
action_id = action:bridge:<bridge_source>:<operation_kind>:<bridge_run_id>
proposing_subject = _system:service:bridge-<bridge_source>
target = upstream artifact id (e.g. "org.gimp.GIMP/x86_64/stable" for Flathub;
                               "library/postgres@sha256:..." for OCI;
                               "linux-image-generic" for distro)
operation_kind = BridgeOperationKind enum value
```

The pipeline branches by `operation_kind`:

- `PACKAGE_FETCH` → §5.3 fetch only; no signature verify; staging area populated; emit `BRIDGE_FETCH_STARTED` / `BRIDGE_FETCH_COMPLETED`.
- `PACKAGE_REPACKAGE` → §5.3 fetch (or use prior staging) + §5.4 verify + §5.5 deceptive-claim check + §5.6 repackage + §5.7 hand-off.
- `METADATA_IMPORT` → §5.3 fetch (metadata only; bounded byte budget — default 16 MiB) + lightweight schema validation; produces a metadata object stored in the S12.1 registry; emit `BRIDGE_METADATA_IMPORTED`.
- `RECIPE_IMPORT` → §5.3 fetch (recipe payload) + lightweight schema validation per S12.1 §6.1 `AppRecipe` shape; produces an `AppRecipe` of `RECIPE_IMPORTED` trust class; emit `BRIDGE_RECIPE_IMPORTED`.

### §5.2 Step 2 — Rate-limit check

Each `BridgeSource` has a per-host rate-limit budget enforced by the bridge runtime:

| Source          | `PACKAGE_FETCH` per hour | `METADATA_IMPORT` per hour | `RECIPE_IMPORT` per hour  |
| --------------- | ------------------------ | -------------------------- | ------------------------- |
| `FLATHUB`       | 60                       | 600                        | 200                       |
| `OCI_REGISTRY`  | n/a (operator-explicit)  | 600                        | n/a (no recipes from OCI) |
| `DISTRO_DEB`    | 30 (bootstrap only)      | 200                        | n/a                       |
| `DISTRO_RPM`    | 30 (bootstrap only)      | 200                        | n/a                       |
| `OTHER_BRIDGED` | 0 (reserved)             | 0 (reserved)               | 0 (reserved)              |

Excess attempts are deferred (queued with exponential backoff up to 24 hours; thereafter dropped). Excess does not produce per-attempt evidence; instead, when a bridge has accumulated `≥ 3` deferrals in a 1-hour window, the bridge emits `BRIDGE_RATE_LIMIT_EXCEEDED` (extended-60M) with the source, the operation_kind, and the rolling counter.

The rate limit is a **backpressure mechanism**, not a security control. Its purpose is to bound the bridge's outbound footprint and to prevent a misconfigured or malicious bridge from saturating the host's network. Per-source budgets are configurable by the operator within bounds (the maximum is the value in the table above; the minimum is 1 per hour for active sources, 0 for `OTHER_BRIDGED`).

### §5.3 Step 3 — Fetch from upstream

The bridge issues outbound HTTP(S) (or upstream-protocol-specific) requests within its `EXPLICIT_ALLOWLIST` network mode. The allowlist is per-source:

- `FLATHUB` — `dl.flathub.org`, `flathub.org` (for `appstream` metadata).
- `OCI_REGISTRY` — the operator-configured registry FQDN(s); never wildcarded.
- `DISTRO_DEB` — the operator-configured distro mirror FQDNs (e.g. `archive.ubuntu.com`, `deb.debian.org`).
- `DISTRO_RPM` — the operator-configured distro mirror FQDNs (e.g. `mirror.fedoraproject.org`).

Fetched bytes are written to `/aios/system/runtime/bridge/<bridge_source>/<bridge_run_id>/staging/`. The staging directory is content-addressed: the bridge computes `BLAKE3(payload)` and stores the artifact under that hash.

Fetch failure modes: connection refused, TLS failure, HTTP 4xx/5xx, byte budget exceeded (4 GiB hard cap per bridge run), timeout (5 minutes default per upstream operation). All retried with exponential backoff up to a per-fetch retry budget; thereafter the operation aborts with `BRIDGE_DEGRADED_UPSTREAM_UNAVAILABLE` (extended-60M). The bridge does NOT emit `REJECTED_UNSIGNED` on fetch failure — fetch failure is a degraded-upstream condition, not a signature failure; the distinction matters for operator-facing diagnostics.

A successful fetch emits `BRIDGE_FETCH_STARTED` (STANDARD_24M) at start and `BRIDGE_FETCH_COMPLETED` (STANDARD_24M) at completion, with the upstream URL, the upstream content hash, and the byte count.

### §5.4 Step 4 — Upstream signature verify

The bridge dispatches verification by `UpstreamSignatureKind`:

| Kind                      | Verification                                                                                                                                                              |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GPG`                     | GPG-detached verify against the source's pinned public keys.                                                                                                              |
| `FLATPAK_OSTREE_GPG`      | Flatpak/OSTree library verify; the OSTree commit signature must be present and verify against the pinned Flathub public keys.                                             |
| `OCI_COSIGN`              | Sigstore cosign verify against the Fulcio root + per-image trust policy; the bridge supports keyless and keyful cosign signatures; the trust policy is in the pinned cat. |
| `DISTRO_RPM_GPG`          | `rpm --checksig` semantics replicated in the bridge using `librpm` (or equivalent) against pinned RPM repository keys.                                                    |
| `DISTRO_DEB_GPG`          | `apt-key`/`gpgv` semantics replicated in the bridge against pinned DEB archive keys; the per-package SHA chain to `Release` / `InRelease` is followed and re-verified.    |
| `DISTRO_DEB_DEBIAN_FORMS` | Debian-format detached signature on the source-package `.dsc`; verified via `gpgv` against pinned keys.                                                                   |
| `SIGNED_TAR`              | Detached `.sig` against pinned vendor key from the bridge's pinned-key catalogue.                                                                                         |
| `UNSIGNED_REJECTED`       | Terminal: bridge does not perform any cryptographic verification; result is `REJECTED_UNSIGNED`.                                                                          |

On signature verify success: emit `BRIDGE_UPSTREAM_SIGNATURE_VERIFIED` (STANDARD_24M) with the signing key id (the upstream's signing key, not the AIOS bridge key) and the signature timestamp.

On signature verify failure (any of: bad signature, missing signature, key not in pinned catalogue, expired signature, revoked key, malformed signature container): emit `BRIDGE_UPSTREAM_SIGNATURE_FAILED` (FOREVER) with the failure reason. The bridge admission result is `REJECTED_UNSIGNED`. The bridge MUST NOT proceed to repackage. There is no operator override path; unsigned upstream is constitutionally inadmissible.

### §5.5 Step 5 — Deceptive-claim check

Before repackaging, the bridge inspects the upstream artifact's metadata (Flatpak `appstream` data, OCI image labels, distro `control` or `spec` files) for claims that would map to AIOS-side trust classes if naively translated. The check is structural:

- If upstream metadata contains a string matching `aios:?root` or `aios:?verified` (case-insensitive, with optional separator) in any field that would be used in the synthesised AIOS manifest's `publisher_trust`, `originating_repository`, or any free-form description that the marketplace surface might display prominently → `REJECTED_DECEPTIVE`.
- If upstream metadata claims affiliation with an AIOS-side publisher catalog entry (`publisher_root_id` matching `pub:*` other than `pub:aios-bridge`) that the bridge cannot confirm via the AIOS publisher catalog → `REJECTED_DECEPTIVE`.
- If upstream metadata claims a `PublisherTrustLevel` value other than the bridge's bound value (`COMMUNITY`) → `REJECTED_DECEPTIVE`.

`REJECTED_DECEPTIVE` is permanent. The rejection is keyed on the upstream content hash; the same artifact will be rejected on every subsequent admission attempt. FOREVER `BRIDGE_TRUST_CLASS_DECEPTION_DETECTED` evidence is emitted with the upstream content hash, the offending field, and the matched pattern.

The check is intentionally conservative — false positives (an upstream description that legitimately mentions "AIOS-verified compatibility" in a non-deceptive way) are accepted as the cost of preventing trust-class confusion. Operators who want to admit such an artifact must re-package it manually and submit it through the standard S11.1 install pipeline as a `COMMUNITY`-tier package under their own publisher identity, which is the constitutional path for non-bridged community contributions.

### §5.6 Step 6 — Repackage with AIOS bridge signing key

For `PACKAGE_REPACKAGE` only. The bridge:

1. Computes `BLAKE3(payload)` over the verified upstream payload (already done at fetch time; reused).
2. Synthesises an AIOS `PackageManifest` per S11.1 §5 with:
   - `package_id = pkg:bridge-<source>:<upstream-name>` (e.g. `pkg:bridge-flathub:org.gimp.GIMP`).
   - `publisher_root_id = pub:aios-bridge`.
   - `package_signing_key_id = pks:aios-bridge:<source>` (e.g. `pks:aios-bridge:flathub`).
   - `publisher_trust = COMMUNITY` — constitutional bound; cannot be raised by any path.
   - `originating_repository = EXTERNAL_BRIDGE` — constitutional bound; cannot be changed.
   - `mirror_semantic = ORIGIN` (the bridge is the origin for the AIOS-shaped package).
   - `kind = APP` (or `ADAPTER` for runtime-adapter bridges; never `INVARIANT_BUNDLE` / `IDENTITY_BUNDLE` / `KERNEL_CANDIDATE` / `POLICY_BUNDLE` / `CAPABILITY_CATALOG_DELTA` — bridges constitutionally cannot synthesise any recovery-only kind, per §6.4 below).
   - `required_sandbox` — the bridged ecosystem's sandbox floor (e.g. S3.2 §9.x Flatpak / Wine / Waydroid / VM profile; per-source default).
   - `declared_capabilities` — derived from the upstream manifest via the S12.1 `ManifestTranslationStrategy` for the source (`FLATPAK_MANIFEST_JSON` for Flathub, etc.); the bridge MUST NOT declare any AI-forbidden or system-admin capability regardless of upstream metadata.
   - `network_manifest` — bounded by the upstream's declared network plus an upper-bound default; the upper bound is per-source (e.g. `MAX_FQDN_FANOUT = 16` for Flathub; lower for OCI metadata-imported recipes).
3. Computes `manifest_canonical_hash` per S11.1 §5.2.
4. Signs the manifest with the AIOS bridge package signing key via the Vault Broker (`KEY_SIGN` capability bound to `vault://aios/bridge/<source>/signing`); the bridge service subject never sees the raw signing key.
5. Records bridge audit metadata in a separate evidence record:

```proto
message BridgeAuditMetadata {
  string bridge_run_id = 1;                    // ULID-26
  BridgeSource source = 2;
  string upstream_url = 3;
  string upstream_content_hash = 4;            // hex_lower(BLAKE3(payload))[:32]
  UpstreamSignatureKind signature_kind = 5;
  string upstream_signing_key_id = 6;          // upstream identifier, not AIOS
  google.protobuf.Timestamp upstream_signed_at = 7;
  google.protobuf.Timestamp bridge_admitted_at = 8;
  string aios_manifest_canonical_hash = 9;     // hex_lower(BLAKE3(JCS(synthesised manifest)))[:32]
  string operator_consent_evidence_pointer = 10; // set only for ADMITTED_WITH_OPERATOR_CONSENT
}
```

6. Emits `BRIDGE_REPACKAGED_WITH_AIOS_KEY` (STANDARD_24M) with the audit metadata.

The synthesised manifest is now a regular S11.1 package and can flow through the standard install pipeline at step 7 below.

### §5.7 Step 7 — Hand off to S11.1 install pipeline

The synthesised manifest is submitted to the S11.1 install pipeline. The pipeline runs as defined in S11.1 §6 from step 1 (fetch — the bridge's staging path is the LOCAL mirror) through step 17 (first-run capability lie audit). The bridge's role ends at the hand-off; the install pipeline's role begins.

At step 4 (publisher state check), the pipeline confirms `publisher_trust = COMMUNITY` and `publisher_root_id = pub:aios-bridge`. At step 6 (manifest field validation), the pipeline confirms the constitutional bounds (no AIOS_ROOT/VERIFIED claim from a bridged package; no recovery-only `kind`; `originating_repository = EXTERNAL_BRIDGE`). At step 11 (approval), the operator approves with `EXACT_ACTION` binding to the synthesised manifest's `manifest_canonical_hash`. The S5.3 approval prompt explicitly displays the bridge audit metadata so the operator sees the upstream provenance.

At step 17 (first-run capability lie audit), if the bridged package exercises a capability not declared in its synthesised manifest, the standard `CAPABILITY_LIE_DETECTED` (FOREVER) fires and the package is quarantined. This is the local audit that always wins over upstream reputation: a Flathub-curated package with high upstream rating that lies about its capabilities at AIOS-side runtime is quarantined like any other community-tier package.

### §5.8 Step 8 — Emit admission evidence

The bridge emits the final admission evidence with the `BridgeAdmissionResult`:

- `ADMITTED_COMMUNITY` → no separate admission evidence (the standard S11.1 `PACKAGE_INSTALLED` covers it, with the bridge audit metadata cross-referenced via the manifest hash).
- `ADMITTED_WITH_OPERATOR_CONSENT` → `BRIDGE_OPERATOR_CONSENT_GRANTED` (STANDARD_24M) plus the standard S11.1 `PACKAGE_INSTALLED`.
- `DEFERRED_NEEDS_REVIEW` → `BRIDGE_DEFERRED_NEEDS_REVIEW` (STANDARD_24M); the package does not enter the install pipeline.
- `REJECTED_UNSIGNED` → `BRIDGE_UPSTREAM_SIGNATURE_FAILED` (FOREVER); already emitted at step 4.
- `REJECTED_DECEPTIVE` → `BRIDGE_DECEPTIVE_REJECTED` (FOREVER) and `BRIDGE_TRUST_CLASS_DECEPTION_DETECTED` (FOREVER); already emitted at step 5.

The evidence chain is fully reconstructable from the records: bridge run id → fetch records → signature verify → deceptive-claim check → repackage → install pipeline → first-run audit. Every record carries the bridge run id, allowing audit to trace a bridge admission end-to-end.

## §6 Per-source policy

The per-source policy table closes which `BridgeOperationKind` values each `BridgeSource` admits and binds the upstream-signature requirements.

### §6.1 `FLATHUB`

- **Admits:** `PACKAGE_FETCH`, `PACKAGE_REPACKAGE`, `METADATA_IMPORT`, `RECIPE_IMPORT`.
- **Upstream signature:** `FLATPAK_OSTREE_GPG` (mandatory). Unsigned Flatpak refs are `UNSIGNED_REJECTED`.
- **Trust ceiling:** `COMMUNITY`.
- **AIOS ecosystem runtime:** the synthesised manifest sets `EcosystemRuntime = RUNTIME_FLATPAK` per S12.1 §3.1; the sandbox floor is the S3.2 Flatpak floor.
- **Translation strategy:** `FLATPAK_MANIFEST_JSON` per S12.1 §3.3 — capabilities derived from the Flatpak `finishes` section.
- **Recipe import:** Flathub `manifest.json` → AIOS `AppRecipe` of trust class `RECIPE_IMPORTED`; full upstream attribution preserved per S12.1 §6.5.
- **Rate limit:** 60 fetches/hour, 600 metadata imports/hour, 200 recipe imports/hour.
- **Allowlist hosts:** `dl.flathub.org`, `flathub.org`.

Flathub is the canonical "rich-bridge" source: package-import + metadata + recipes are all admitted because Flathub's structure (well-defined manifests, OSTree-bound signatures, public infrastructure) supports the full bridge pipeline.

### §6.2 `OCI_REGISTRY`

- **Admits:** `METADATA_IMPORT` only.
- **Does NOT admit:** `PACKAGE_FETCH` and `PACKAGE_REPACKAGE` are not auto-admitted. The OCI ecosystem is too heterogeneous (private registries, partial signatures, container layers with broad capabilities by default) to safely auto-import packages into an AIOS COMMUNITY-tier admission. Operators who want an OCI image on AIOS must explicitly initiate a package fetch via a typed action (`bridge.oci.fetch_explicit`) that is gated by an `EXACT_ACTION` approval per S5.3 and that runs through a separate admission pipeline outside the auto-bridge. That explicit-fetch pipeline is queued for `02_marketplace.md` consolidation; this contract names only the auto-bridge boundary.
- **Upstream signature for `METADATA_IMPORT`:** `OCI_COSIGN` if present; if absent, the metadata is imported as `UNSIGNED_REJECTED` and discarded — even metadata-only imports require a valid signature in the OCI case because metadata can carry executable instructions (e.g. `Dockerfile` snippets in image labels) that downstream tools may execute.
- **Trust ceiling:** `COMMUNITY` (via the explicit-fetch pipeline; not via auto-bridge).
- **Recipe import:** not admitted from OCI (the OCI ecosystem does not have an AIOS-recipe-shaped model).
- **Rate limit:** 600 metadata imports/hour. No package fetch budget at the auto-bridge layer.
- **Allowlist hosts:** the operator-configured registry FQDN(s); never wildcarded.

OCI's policy is intentionally conservative: metadata-only at the auto-bridge layer, with operator-explicit fetch as the only path for actual packages. The reason is that OCI registries are the primary surface for container-supply-chain attacks (typosquatting, dependency confusion, layer poisoning) and the auto-bridge cannot defend the operator without their explicit decision per package.

### §6.3 `DISTRO_DEB` and `DISTRO_RPM`

- **Admits:** `PACKAGE_FETCH` for **first-boot system bootstrap only**.
- **Does NOT admit:** user-facing application installs from distro repos. Once AIOS is past first-boot, distro repos are not used as a source for new applications; the operator uses Flathub, the AIOS marketplace, or explicit OCI fetches instead.
- **Upstream signature:** `DISTRO_DEB_GPG` / `DISTRO_RPM_GPG` (mandatory). Unsigned distro packages are `UNSIGNED_REJECTED`.
- **Trust ceiling:** `COMMUNITY`. The synthesised manifest's `kind = APP` (system-bootstrap context only).
- **Recovery boundary:** distro repo fetches during first-boot bootstrap occur under recovery-mode-equivalent system bootstrap (S9.1). After first-boot, the bridge's distro-repo allowlist is empty and any attempted fetch is rejected at step 3 with `BRIDGE_DEGRADED_UPSTREAM_UNAVAILABLE` (the hosts are simply unreachable from the bridge's sandbox).
- **Rate limit:** 30 fetches/hour during bootstrap; 0 thereafter.
- **Allowlist hosts during bootstrap:** the operator-configured (or installer-default) distro mirror FQDNs.
- **Translation strategy:** none — bootstrap packages are admitted to AIOS at install time without going through the S12.1 four-phase setup; the AIOS install pipeline still applies (signature, content hash, manifest field, sandbox profile validation).

The distro-repo policy reflects the constitutional position of distro packages in AIOS: they are the substrate AIOS sits on top of, used to establish the L1 generic-fallback kernel and the L1 host bootstrap (per Rev.1 §6 layer rules). They are never user-facing apps; that is L6 / L10's job through Flathub or the AIOS marketplace. Mixing the two would re-introduce the legacy distro-package-as-app coupling that AIOS's per-app sandbox model is designed to escape.

### §6.4 `OTHER_BRIDGED`

Reserved enum slot. No admissions are processed under `OTHER_BRIDGED` in this contract revision. A future revision adding a new bridge MUST accompany the activation with:

1. A versioned spec change adding the new `BridgeSource` value (renaming or repurposing `OTHER_BRIDGED` would break the closed-enum guarantee and is forbidden).
2. A per-source policy entry analogous to §6.1–§6.3 above.
3. A pinned-public-keys catalogue entry signed by the AIOS bridge publisher.
4. Per-source rate-limit budgets.
5. AIOS root co-signed activation evidence.

Until such a spec change, any bridge run reporting `BridgeSource = OTHER_BRIDGED` is rejected at step 1 of the pipeline with `REJECTED_DECEPTIVE` (sub-reason `RESERVED_SOURCE_USED`).

### §6.5 No bridge admits recovery-only kinds

A bridge constitutionally cannot synthesise a manifest with any recovery-only `PackageKind` (S11.1 §3.4: `INVARIANT_BUNDLE`, `POLICY_BUNDLE`, `IDENTITY_BUNDLE`, `KERNEL_CANDIDATE`, `CAPABILITY_CATALOG_DELTA`). These kinds require recovery-mode + AIOS_ROOT trust + S9.1 RecoveryMutableScope; the bridge's `COMMUNITY` ceiling is incompatible with all three. A bridge that attempts to emit such a manifest is rejected at step 6 of the bridge pipeline with `REJECTED_DECEPTIVE` (sub-reason `BRIDGE_RECOVERY_ONLY_KIND_ATTEMPTED`); FOREVER `BRIDGE_DECEPTIVE_REJECTED` evidence.

## §7 Metadata import (separate from package import)

Metadata import is the lightweight bridge operation that brings upstream descriptive content into AIOS without bringing the upstream package payload itself. Metadata-only imports power the L7 marketplace surface's "browse what's available" experience without committing the operator to any installation.

### §7.1 What is metadata?

Metadata fields the bridge may import:

- App / package name, version, summary, description, screenshots URL, homepage URL.
- Upstream rating / star count (where the upstream surfaces it; e.g. Flathub's installs counter).
- Upstream category / keywords / classification.
- Upstream license declaration.
- Upstream changelog / release notes.
- Upstream attribution chain (the upstream's publisher, the upstream's original source URL).

Metadata fields the bridge MUST NOT import:

- Executable code, install hooks, post-install scripts.
- Capability declarations represented as live AIOS capability ids (those are derived only at `PACKAGE_REPACKAGE` time via the `ManifestTranslationStrategy`; metadata import is not allowed to short-circuit the strategy).
- AIOS-side trust class claims (see §5.5 deceptive-claim check; trust class claims in upstream metadata trigger `REJECTED_DECEPTIVE`).
- Operator personal data the upstream may have collected (e.g. download counters with operator-identifying fields). The bridge filters these at import.

### §7.2 Source attribution

Every imported metadata object carries a structured attribution:

```proto
message MetadataAttribution {
  BridgeSource source = 1;
  string upstream_id = 2;                     // upstream-native id (Flathub app id, OCI image ref, distro package name)
  string upstream_url = 3;
  google.protobuf.Timestamp imported_at = 4;
  string bridge_run_id = 5;
  string aios_metadata_object_id = 6;         // content-addressed id of the imported metadata in AIOS storage
}
```

The L7 marketplace surface MUST display the attribution on every operator-facing listing of bridge-sourced content. Operators see "from Flathub" / "from Docker Hub" / "imported on YYYY-MM-DD" alongside the listing; this is the consent-via-disclosure side of the honesty principle (S12.1 §7).

### §7.3 Trust never auto-grants from metadata

Imported metadata is metadata. It does not auto-grant any AIOS-side trust class to the corresponding (or any) package. Specifically:

- An operator viewing a Flathub-imported metadata listing does not implicitly accept the Flathub package; viewing is not consenting.
- A metadata listing's upstream rating (e.g. "10000 installs on Flathub") is informational; AIOS does not promote a high-rated upstream listing to AIOS_VERIFIED trust.
- A metadata listing whose upstream attribution claims an AIOS-side trust class is rejected at import per §5.5 deceptive-claim check.

The reason is: metadata is the primary attack surface for trust laundering. Importing metadata uncritically and surfacing it as if it were AIOS-curated would let an upstream attacker influence AIOS-side trust by influencing upstream metadata. The metadata is shown to the operator with attribution; the trust class on any actual AIOS-side install of the corresponding package remains `COMMUNITY` (or, for non-bridged AIOS-native publishers, whatever S11.1 grants).

### §7.4 Metadata storage and lifecycle

Imported metadata is stored in `/aios/system/runtime/bridge/<source>/metadata/<aios_metadata_object_id>` as a content-addressed JCS object. Metadata is refreshed by re-import (a new `bridge_run_id`); the old metadata object is retained for audit and can be diffed against the new one to detect upstream changes.

A metadata object whose content hash changes between imports without a corresponding upstream version bump is flagged with `BRIDGE_METADATA_DRIFT_DETECTED` (extended-60M) — this is a weak signal of upstream tampering or upstream source instability. The bridge does not auto-blacklist on metadata drift; the signal feeds the L9 admin surface for operator review.

## §8 Recipe import

Recipe import is the AIOS-specific bridge operation that brings upstream community knowledge about how to run a piece of software into the S12.1 community recipe registry as a `RECIPE_IMPORTED`-trust-class entry.

### §8.1 What is a recipe (cited from S12.1)?

Per S12.1 §6.1, an `AppRecipe` is a content-addressed, signed object that bundles `EcosystemRuntime + EcosystemHonestyClass + ManifestTranslationStrategy + SandboxProfile + NetworkOutboundManifest + declared_capabilities + upstream_attribution`. A recipe is **metadata about how to run an app**, not an app itself. Local Phase A and Phase C audits remain authoritative regardless of recipe trust class.

### §8.2 Recipe sources for import

| Upstream source         | `EcosystemRuntime`        | `ManifestTranslationStrategy` | Honest scope                                                          |
| ----------------------- | ------------------------- | ----------------------------- | --------------------------------------------------------------------- |
| ProtonDB + WineHQ AppDB | `RUNTIME_WINDOWS_PROTON`  | `PROTON_RECIPE`               | Wine-compatibility data with attribution chain.                       |
| Flathub `manifest.json` | `RUNTIME_FLATPAK`         | `FLATPAK_MANIFEST_JSON`       | Direct 1:1 mapping; well-defined upstream shape.                      |
| Snapcraft store         | (deferred per S12.1 §6.5) | `SNAPCRAFT_YAML`              | Snapcraft.yaml + store metadata; not currently in the bridge sources. |
| AUR PKGBUILDs           | (deferred per S12.1 §6.5) | n/a                           | AUR script → AIOS recipe; not currently in the bridge sources.        |

This contract admits `PROTONDB`-and-`FLATHUB`-sourced recipe imports under the `FLATHUB` bridge source (Flatpak manifests) and a future per-source extension for ProtonDB. AUR and Snapcraft recipe imports are deferred to a later contract revision; the S12.1 §6.5 table defines them but the bridge mechanics are queued.

### §8.3 Recipe import workflow

1. The bridge fetches the upstream recipe payload (Flathub `manifest.json`, ProtonDB recipe entry, etc.) within its `PACKAGE_FETCH` allowlist for the source.
2. The bridge validates the upstream signature (Flathub OSTree-bound GPG for Flathub manifests; ProtonDB does not surface signatures and is therefore admitted only via a per-source HTTPS-pinned-CA mechanism + content-hash-attestation by AIOS bridge — a weakening that is recorded explicitly in the `RecipeAttribution`).
3. The bridge translates the upstream recipe into an AIOS `AppRecipe` shape per S12.1 §6.1, setting:
   - `trust_class = RECIPE_IMPORTED`.
   - `upstream_attribution = ["protondb:<recipe_id>", "flathub:<app_id>", ...]`.
   - `contributor_subject_canonical_id = _system:service:bridge-<source>`.
4. The bridge signs the recipe with the AIOS bridge per-source signing key.
5. The recipe is published to S11.1 `AIOS_COMMUNITY_REPO` per S12.1 §6.4 (which is the standard publish path).
6. `BRIDGE_RECIPE_IMPORTED` (STANDARD_24M) evidence is emitted with the upstream attribution.

### §8.4 Local audit always wins

The recipe is metadata-only. When an operator installs an app using an imported recipe:

- Phase A pre-flight observation (S12.1 §4.1) runs locally regardless of recipe trust class. The recipe's claimed `SandboxProfile` is a starting hypothesis; the local observation produces the actual `ObservedBehavior` summary.
- Phase B manifest proposal (S12.1 §4.2) generates a fresh signed proposal; the recipe seeds the proposer's input but does not bypass it.
- Phase C first-run capability lie audit (S12.1 §4.3 / S11.1 §G) runs locally on every install; a high-upstream-reputation recipe with a local capability-lie event is quarantined like any other.

This is the constitutional rule from §1: **local Phase A always wins**. Imported recipes are reusable AI starting points, not authority transfers.

## §9 Bridge reputation and auto-blacklist

Bridges themselves are subject to reputation tracking, just as the recipes they import are. A bridge that misbehaves (fetches malicious content; imports deceptive metadata; produces a recurring stream of `REJECTED_DECEPTIVE` admissions) accumulates reputation events and may be auto-blacklisted.

### §9.1 Per-bridge reputation counters

Each bridge instance (per `BridgeSource`) carries:

- `successful_admissions_count` — number of `ADMITTED_COMMUNITY` outcomes.
- `rejected_unsigned_count` — number of `REJECTED_UNSIGNED` outcomes.
- `rejected_deceptive_count` — number of `REJECTED_DECEPTIVE` outcomes.
- `rate_limit_exceeded_count` — number of `BRIDGE_RATE_LIMIT_EXCEEDED` events.
- `degraded_upstream_count` — number of `BRIDGE_DEGRADED_UPSTREAM_UNAVAILABLE` events.
- `breakout_attempted_count` — number of `BRIDGE_DECEPTIVE_REJECTED` with sub-reason `SANDBOX_BREAKOUT_ATTEMPTED`.

### §9.2 Auto-blacklist conditions

A bridge is auto-blacklisted when any of:

- `rejected_deceptive_count / total_admission_attempts > 0.10` over a rolling 7-day window — a bridge that produces deceptive admissions more than 10% of the time is structurally untrustworthy.
- `breakout_attempted_count >= 1` ever — a single sandbox breakout attempt is sufficient evidence that the bridge has been compromised.
- `rate_limit_exceeded_count >= 10` over a rolling 24-hour window combined with `degraded_upstream_count >= 100` — pattern consistent with a bridge being used to amplify outbound traffic.

Auto-blacklist:

- Emits `BRIDGE_BLACKLISTED` (FOREVER) with the source, the trigger condition, and the rolling counter values.
- Stops further admissions from the bridge until operator review.
- Existing admitted packages from the bridge remain installed (no retroactive quarantine), but the AIOS-bridge-publisher's per-source signing key for that bridge is **rotated** at next operator opportunity to prevent further admissions from a potentially compromised bridge.

### §9.3 Operator review and re-enable

The L9 admin surface presents blacklisted bridges to the operator with the FOREVER evidence record and the reputation counters. The operator can re-enable a bridge through an `EXACT_ACTION` approval bound to the `BRIDGE_BLACKLISTED` evidence id, with `ApprovalStrength = STRONG`. Re-enable resets the counters to zero and emits `BRIDGE_BLACKLIST_LIFTED` (FOREVER) with the operator's identity and the reset.

A bridge that is re-enabled and then re-triggers any auto-blacklist condition is **permanently deplatformed**: the AIOS bridge publisher's per-source signing key is revoked (per S11.1 §11 publisher key rotation, with `TakedownReason = SUPPLY_CHAIN_COMPROMISE`); the bridge package is uninstalled; the source's allowlist hosts are removed from the bridge sandbox's `EXPLICIT_ALLOWLIST`. Re-introducing a permanently deplatformed bridge requires a versioned spec change.

## §10 Adversarial robustness

This section enumerates the named adversarial vectors this contract defends against and the named defense for each.

### §10.1 Upstream signature compromise

**Vector.** An upstream's signing key is compromised; the attacker signs malicious upstream packages that pass upstream signature verification but contain malware.

**Defense.** AIOS-side defenses are layered:

1. The AIOS bridge-signing layer cross-checks upstream signature **and** synthesises an AIOS manifest at `COMMUNITY` trust regardless. Upstream signature passing does not promote the package to AIOS-VERIFIED — the package still flows through the standard S11.1 install pipeline at `COMMUNITY` trust.
2. The S11.1 first-run capability lie audit (cited from §5.7) catches any capability the upstream package exercises but did not declare in its synthesised AIOS manifest. A malicious package that exercises broader capabilities than its upstream-derived manifest declares is quarantined.
3. The S3.2 sandbox floor (INV-017) bounds the worst-case damage: a malicious bridged package cannot escalate beyond the bridge ecosystem's sandbox floor regardless of upstream signature.
4. Rotation: when an upstream's compromise is announced, the AIOS bridge updates its pinned-public-keys catalogue (via AIOS-bridge-publisher-signed delta); subsequent admissions verifying against the old key are rejected with `BRIDGE_UPSTREAM_SIGNATURE_FAILED`.

The defense does NOT rely on AIOS magically detecting an upstream compromise before the upstream announces it. It relies on the local audit + sandbox floor catching what the upstream's compromise lets through.

### §10.2 Supply-chain attack on bridged packages

**Vector.** An attacker compromises a Flathub publisher's build pipeline and uploads a malicious version of a popular package; the package is signed by the upstream's compromised key.

**Defense.** Same layered defense as §10.1. Additionally:

- The AIOS bridge surfaces upstream version changes prominently in the L7 marketplace bridge listing; an unexpected version bump on a stable-channel package is operator-visible.
- A bridged package whose synthesised manifest's `manifest_canonical_hash` changes between releases triggers fresh approval per S5.3 `EXACT_ACTION` binding (the prior approval is bound to the prior canonical hash; a new hash voids the prior binding); the operator must re-approve and the prompt explicitly indicates "this is a new version of an existing package".
- The S11.1 §15.2 replay-of-older-signed-packages defense applies: bridged packages have monotonic version ordering; downgrades require explicit `STRONG` approval.

### §10.3 Deceptive metadata claiming higher trust

**Vector.** An upstream's metadata is crafted to claim AIOS-side trust class affiliation it does not have (e.g. a Flathub package's description string contains "officially AIOS-verified by AIOS root" to influence an operator's perception).

**Defense.** §5.5 deceptive-claim check rejects at admission with `REJECTED_DECEPTIVE` and FOREVER `BRIDGE_TRUST_CLASS_DECEPTION_DETECTED`. The upstream-metadata-cannot-grant-AIOS-side-trust mapping is structural: metadata fields never participate in the `publisher_trust` decision; that field is set by the bridge itself from the constitutional bound (`COMMUNITY`).

### §10.4 Rate-limit evasion

**Vector.** A malicious or misconfigured bridge attempts to amplify its outbound traffic by issuing many fetches per hour, e.g. to participate in a DDoS or to exhaust the operator's bandwidth.

**Defense.** Per-source rate-limit budgets (§5.2) are hard-enforced by the bridge runtime, not by the bridge code itself. The runtime is part of the AIOS service that hosts the bridge; the bridge cannot bypass it. Rate-limit violations beyond the deferral window (§5.2) emit `BRIDGE_RATE_LIMIT_EXCEEDED` (extended-60M) and feed the auto-blacklist counter (§9.2). The sandbox `outbound_byte_budget` (§4.3, 4 GiB per bridge run) is a secondary cap at the byte level; even a bridge that cycles fetches at the rate-limit boundary cannot exceed 4 GiB of outbound per run.

### §10.5 Bridge-as-malware-distributor

**Vector.** A bridge implementation itself is malicious or compromised — e.g. a bridge package on the AIOS marketplace that, despite its `pub:aios-bridge` provenance, is actually a third-party impostor, or a bridge that has been compromised through a supply-chain attack on the AIOS bridge publisher itself.

**Defense.** Multiple layers:

- The AIOS bridge publisher (`pub:aios-bridge`) is signed by the AIOS root key per the standard three-tier chain (S11.1 §4). An impostor cannot mint a forged `pub:aios-bridge` manifest because the publisher catalog is AIOS-root-signed; a forged catalogue would fail signature verification at the host.
- The bridge sandbox (§4.3) bounds what a compromised bridge can do: no host filesystem access outside the bridge staging area; no GPU; no inbound network; no user namespace; no ptrace. Even a fully-compromised bridge is confined to producing AIOS manifests at `COMMUNITY` trust which the local audit then catches.
- Reputation tracking (§9) detects misbehaving bridges and auto-blacklists them.
- Permanent deplatform (§9.3) for repeat offenders ensures that a compromised bridge cannot be silently re-enabled.

### §10.6 Bridged package claiming AIOS_VERIFIED in metadata

**Vector.** An upstream package's manifest, description, or sidecar metadata explicitly claims `AIOS_VERIFIED` trust on the bridged side.

**Defense.** §5.5 deceptive-claim check structural rule: any string match for `aios:?root` or `aios:?verified` in any prominently-displayed metadata field, or any explicit `PublisherTrustLevel` claim other than `COMMUNITY`, triggers `REJECTED_DECEPTIVE`. The rejection is **permanent** and keyed on the upstream content hash — the same artifact will be rejected on every subsequent admission attempt; FOREVER `BRIDGE_TRUST_CLASS_DECEPTION_DETECTED` evidence persists.

The rule is intentionally strict; an upstream that legitimately wants to mention AIOS compatibility must use a different phrasing (e.g. "compatible with AIOS via Flathub bridge" — which does not match the prohibited patterns). The asymmetry — easier to be rejected as deceptive than to claim higher trust — is consistent with the constitutional honesty principle: the cost of a false-positive rejection is an operator workaround; the cost of a false-negative admission is supply-chain trust-laundering on the AIOS host.

### §10.7 Imported recipe as authority transfer

**Vector.** A high-upstream-reputation recipe is imported (e.g. a Flathub manifest with thousands of installs) and the operator assumes the recipe's reputation maps to AIOS-side trust.

**Defense.** Local Phase A and Phase C audits (cited from S12.1) always run, regardless of recipe trust class. The `RECIPE_IMPORTED` trust class is surfaced at the L7 marketplace prompt; the upstream attribution is preserved; the operator sees "imported from Flathub" alongside the recipe. There is no UI surface that elevates an imported recipe to look like an AIOS-curated recipe (`RECIPE_AIOS_CURATED`); the trust class boundary is structural per S12.1 §3.5.

### §10.8 Bridge-staging filesystem poisoning

**Vector.** An attacker who can write to the bridge's staging area attempts to substitute a payload after the upstream signature is verified but before the AIOS manifest is synthesised.

**Defense.** The staging area is per-bridge-run (§4.3) and lives under `/aios/system/runtime/bridge/<source>/<bridge_run_id>/staging`. Only the bridge service subject can write there (filesystem `allow_write` enforced by S3.2). Between fetch and synthesise, the bridge maintains the upstream content hash in memory and re-computes it before manifest synthesis; any mismatch between the in-memory hash and the on-disk re-computed hash aborts with `BRIDGE_DECEPTIVE_REJECTED` (sub-reason `STAGING_HASH_MISMATCH`). Additionally, the staging area is on `tmpfs` per the sandbox profile, so a host-side attacker would need to compromise the AIOS service hosting the bridge — at which point the AIOS host itself is compromised and bridge integrity is not the relevant defense layer (S11.1 §4.1 firmware-pin and recovery boundary apply).

## §11 Telemetry contract

Bounded-cardinality metrics. Bridge run id, upstream content hash, upstream signing key id, and operator subject id are NEVER labels — they appear in evidence records, never as Prometheus labels.

| Metric                                        | Type      | Labels (closed)                                                                                                   | Cardinality budget |
| --------------------------------------------- | --------- | ----------------------------------------------------------------------------------------------------------------- | ------------------ |
| `bridge_admission_total`                      | counter   | `source` (closed `BridgeSource`), `result` (closed `BridgeAdmissionResult`)                                       | ≤ 25               |
| `bridge_operation_total`                      | counter   | `source`, `operation_kind` (closed `BridgeOperationKind`), `outcome` (success / deferred / failed)                | ≤ 60               |
| `bridge_upstream_signature_total`             | counter   | `source`, `kind` (closed `UpstreamSignatureKind`), `result` (verified / failed / unsigned)                        | ≤ 120              |
| `bridge_rate_limit_exceeded_total`            | counter   | `source`, `operation_kind`                                                                                        | ≤ 20               |
| `bridge_degraded_upstream_total`              | counter   | `source`                                                                                                          | ≤ 5                |
| `bridge_metadata_imported_total`              | counter   | `source`                                                                                                          | ≤ 5                |
| `bridge_recipe_imported_total`                | counter   | `source`                                                                                                          | ≤ 5                |
| `bridge_blacklisted_total`                    | counter   | `source`, `trigger` (closed: deceptive_threshold / breakout_single / amplification_pattern)                       | ≤ 15               |
| `bridge_repackage_duration_seconds`           | histogram | `source`                                                                                                          | ≤ 5 (× buckets)    |
| `bridge_trust_class_deception_detected_total` | counter   | `source`, `pattern` (closed: aios_root_claim / aios_verified_claim / publisher_root_claim / explicit_trust_field) | ≤ 20               |
| `bridge_run_active_count`                     | gauge     | `source`                                                                                                          | ≤ 5                |
| `bridge_repackaged_with_aios_key_total`       | counter   | `source`                                                                                                          | ≤ 5                |

NEVER as labels: `bridge_run_id`, `upstream_content_hash`, `upstream_signing_key_id`, `aios_manifest_canonical_hash`, `subject_id`, `operator_id`, `upstream_url`.

Total cardinality budget: ≤ 300 active label tuples across all bridge metrics.

## §12 Evidence record types (queue for S3.1)

Twelve new record types are queued for the next S3.1 consolidation. Every record carries `bridge_run_id`, `source`, and the relevant operation_kind / signature_kind / admission_result.

| Record type                             | Retention class | Trigger                                                                                                                     |
| --------------------------------------- | --------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `BRIDGE_FETCH_STARTED`                  | STANDARD_24M    | Bridge fetch operation started (§5.3, step 3 entry).                                                                        |
| `BRIDGE_FETCH_COMPLETED`                | STANDARD_24M    | Bridge fetch operation completed successfully; carries upstream content hash and byte count.                                |
| `BRIDGE_UPSTREAM_SIGNATURE_VERIFIED`    | STANDARD_24M    | Upstream signature verification passed (§5.4); carries upstream signing key id and signature timestamp.                     |
| `BRIDGE_UPSTREAM_SIGNATURE_FAILED`      | FOREVER         | Upstream signature verification failed or upstream is `UNSIGNED_REJECTED` (§5.4); carries failure reason.                   |
| `BRIDGE_REPACKAGED_WITH_AIOS_KEY`       | STANDARD_24M    | Bridge synthesised an AIOS manifest at `COMMUNITY` trust and signed it with the AIOS bridge per-source key (§5.6).          |
| `BRIDGE_DECEPTIVE_REJECTED`             | FOREVER         | Bridge admission rejected with `REJECTED_DECEPTIVE` (§5.5 / §6.4 / §6.5 / §10.6); carries sub-reason and offending field.   |
| `BRIDGE_RATE_LIMIT_EXCEEDED`            | EXTENDED_60M    | Bridge accumulated `≥ 3` deferrals in a 1-hour window (§5.2); carries source, operation_kind, rolling counter.              |
| `BRIDGE_METADATA_IMPORTED`              | STANDARD_24M    | Metadata-only import completed (§5.1, §7); carries `MetadataAttribution`.                                                   |
| `BRIDGE_RECIPE_IMPORTED`                | STANDARD_24M    | Recipe import completed (§5.1, §8); carries upstream attribution and recipe canonical id.                                   |
| `BRIDGE_BLACKLISTED`                    | FOREVER         | Bridge auto-blacklisted (§9.2); carries source, trigger condition, counter snapshot.                                        |
| `BRIDGE_DEGRADED_UPSTREAM_UNAVAILABLE`  | EXTENDED_60M    | Bridge fetch failed after retry budget exhausted (§5.3); carries source, upstream URL, last error.                          |
| `BRIDGE_TRUST_CLASS_DECEPTION_DETECTED` | FOREVER         | Deceptive-claim check detected an AIOS-side trust class claim in upstream metadata (§5.5 / §10.6); carries pattern matched. |

These record types extend S3.1 §4 `RecordType` enum at the next S3.1 consolidation. Until then, this contract treats them as queued; emitters write them via the existing `EvidenceLog.Append` RPC with the proposed enum value reserved.

## §13 Cross-spec dependencies

| Spec                                           | Direction           | What this contract relies on / produces                                                                                                                                                                          |
| ---------------------------------------------- | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| L0 INV-002, INV-008, INV-013, INV-014, INV-017 | constraint          | AI proposes never executes; default-deny; AI cannot perform system admin; no proof no completion; sandbox floor constitutional. INV-014 specifically forbids unsigned-upstream admission masquerading as `REAL`. |
| S11.1 Repository Model                         | consumer + producer | Bridges produce `EXTERNAL_BRIDGE`-origin packages at `COMMUNITY` trust; this contract deepens S11.1 §14 without redefining the trust chain. The AIOS bridge publisher is registered per S11.1 §4.2.              |
| S12.1 App Runtime Model                        | consumer            | The bridge selects an `EcosystemRuntime` from the closed S12.1 vocabulary at repackage time; recipe imports populate the S12.1 community recipe registry as `RECIPE_IMPORTED`; local Phase A/C always wins.      |
| S5.3 Approval Mechanics                        | consumer            | `EXACT_ACTION` bindings on bridged-package install (operator approves the synthesised manifest hash); `ADMITTED_WITH_OPERATOR_CONSENT` outcomes use the same binding mechanism.                                  |
| S3.2 Sandbox Composition                       | consumer            | Each bridge run is an `ISOLATED_SANDBOX` instance with the per-source sandbox profile floor; constitutional `INV-017` floor applies; bridge package's own runtime is bounded by §4.3.                            |
| S0.1 Action Envelope Lifecycle                 | consumer            | Every bridge operation is a typed action (`bridge.fetch`, `bridge.repackage`, `bridge.import_metadata`, `bridge.import_recipe`); subjects, targets, FSM transitions follow S0.1.                                 |
| S3.1 Evidence Log                              | producer            | Twelve new record types queued (§12); existing `Append` RPC consumed.                                                                                                                                            |
| S8.1 Network Policy                            | consumer            | Each bridge sandbox carries its own `NetworkOutboundManifest` allowlisting upstream FQDNs; runtime-time enforcement by L8.                                                                                       |
| S2.3 Policy Kernel                             | consumer            | `EvaluatePolicy` at the bridge-package install step (which is S11.1 step 10); AI-direct-install hard-deny applies.                                                                                               |
| S9.1 Recovery Boundary                         | consumer            | `DISTRO_DEB` and `DISTRO_RPM` package fetches at first-boot bootstrap occur within recovery-mode-equivalent system bootstrap; post-bootstrap distro hosts are not in the allowlist.                              |
| L10 marketplace (`02`)                         | producer            | This contract defines what the marketplace surface displays for bridge-sourced apps (attribution, trust class, honesty class); the `02` UX consumes the metadata and recipe registry entries.                    |
| L10 repository model (`01`)                    | producer            | This contract names the AIOS bridge publisher, its constitutional bound, and the per-source signing keys; rotation follows S11.1 §11 publisher key rotation discipline.                                          |

## §14 Worked examples

### §14.1 Example 1 — Flathub package admitted as COMMUNITY

**Setup.**

- Operator: `alice` (`HUMAN_USER`, primary group = `home`, session class = `STRONG`).
- Upstream artifact: `org.gimp.GIMP/x86_64/stable` from Flathub (`dl.flathub.org`).
- Bridge: `_system:service:bridge-flathub` (the AIOS Flathub bridge instance running under §4.3 sandbox).
- Bridge run id: `brun_01HZK...` (ULID-26).

**Trace.**

1. Operator initiates "browse Flathub" in the L7 marketplace; selects GIMP; clicks "install".
2. L7 emits `bridge.repackage` typed action with `BridgeSource = FLATHUB`, target = `org.gimp.GIMP/x86_64/stable`, proposing subject = `_system:service:bridge-flathub`. Action envelope per S0.1.
3. **Step 1 (operation kind dispatch).** `BridgeOperationKind = PACKAGE_REPACKAGE`. Pipeline branches to fetch + verify + deceptive-claim + repackage + hand-off.
4. **Step 2 (rate-limit check).** Flathub fetches in last hour: 12. Budget: 60. Proceed.
5. **Step 3 (fetch).** Bridge fetches OSTree commit + Flatpak manifest from `dl.flathub.org` within `EXPLICIT_ALLOWLIST`. `BRIDGE_FETCH_STARTED` (STANDARD_24M) and `BRIDGE_FETCH_COMPLETED` (STANDARD_24M) emitted. Upstream content hash: `0xab12...` (BLAKE3, 32 hex chars).
6. **Step 4 (upstream signature verify).** `UpstreamSignatureKind = FLATPAK_OSTREE_GPG`. Bridge runs the OSTree-bound GPG verify against the pinned Flathub public keys. Verified. `BRIDGE_UPSTREAM_SIGNATURE_VERIFIED` (STANDARD_24M) emitted with upstream signing key id `flathub@flathub.org`.
7. **Step 5 (deceptive-claim check).** Flathub `appstream` data inspected; no `aios:root` or `aios:verified` patterns found; no `PublisherTrustLevel` claim in metadata. Pass.
8. **Step 6 (repackage).** Bridge synthesises manifest:
   - `package_id = pkg:bridge-flathub:org.gimp.GIMP`.
   - `version = 2.10.36`.
   - `kind = APP`.
   - `publisher_trust = COMMUNITY` (constitutional bound).
   - `publisher_root_id = pub:aios-bridge`.
   - `package_signing_key_id = pks:aios-bridge:flathub`.
   - `originating_repository = EXTERNAL_BRIDGE`.
   - `mirror_semantic = ORIGIN`.
   - `required_sandbox` = S3.2 §9.x Flatpak floor (root = NO_ACCESS, network = EXPLICIT_ALLOWLIST, secrets = BROKER_ONLY).
   - `declared_capabilities` = derived from Flatpak `finishes` section via `FLATPAK_MANIFEST_JSON` strategy: `[filesystem.read.user, filesystem.write.user, x11.display, audio.playback]`.
   - `network_manifest` = `[HOST_FQDN: download.gimp.org]`, fan-out ≤ 4.
9. Bridge signs the manifest via vault `KEY_SIGN` on `pks:aios-bridge:flathub`. `BRIDGE_REPACKAGED_WITH_AIOS_KEY` (STANDARD_24M) emitted with `BridgeAuditMetadata`.
10. **Step 7 (hand-off to S11.1).** S11.1 install pipeline runs:
    - Trust chain verifies: `pks:aios-bridge:flathub` ← `pub:aios-bridge` ← AIOS root.
    - Publisher state: `pub:aios-bridge` at AIOS_ROOT trust (the publisher); manifest at COMMUNITY (the package). No deplatform.
    - Content hash matches.
    - Manifest field validation: all enum values closed; `publisher_trust = COMMUNITY` matches the bridged-package constitutional bound.
    - Sandbox profile validates against host's S3.2 capabilities.
    - Capability declarations resolve in L5 catalog.
    - Network manifest validates per S8.1.
    - Policy kernel returns `REQUIRE_APPROVAL`.
    - S5.3 prompt to alice via `KDE_NATIVE_PROMPT`; prompt displays:
      - "Source: Flathub (bridge admission)".
      - "Trust class: COMMUNITY".
      - "Honesty class: FULLY_SUPPORTED" (from S12.1 RUNTIME_FLATPAK).
      - The four declared capabilities as plain-language summary.
      - The single FQDN in the network manifest.
    - Alice approves. `EXACT_ACTION` binding consumed.
    - Atomic install + capability bindings issued.
    - `PACKAGE_INSTALLED` (STANDARD_24M) emitted with cross-reference to `BRIDGE_REPACKAGED_WITH_AIOS_KEY`.
    - First-run capability lie audit runs for 60 s; observed = `{filesystem.read.user, x11.display, audio.playback}` ⊆ declared. `PACKAGE_AUDIT_PASSED` (STANDARD_24M).
11. **Final state.** `pkg:bridge-flathub:org.gimp.GIMP@2.10.36` `ACTIVE` under alice's user scope. `BridgeAdmissionResult = ADMITTED_COMMUNITY`. Bridge reputation counter `successful_admissions_count` incremented.

**Evidence chain.** `BRIDGE_FETCH_STARTED` → `BRIDGE_FETCH_COMPLETED` → `BRIDGE_UPSTREAM_SIGNATURE_VERIFIED` → `BRIDGE_REPACKAGED_WITH_AIOS_KEY` → S11.1 `PACKAGE_VERIFIED` → S11.1 `PACKAGE_INSTALLED` → S11.1 `PACKAGE_AUDIT_PASSED`. All linked by `bridge_run_id` and `aios_manifest_canonical_hash`.

### §14.2 Example 2 — OCI metadata-only import

**Setup.**

- Operator: `alice` (browsing the L7 marketplace's "container catalogue" surface).
- Upstream registry: `ghcr.io` (operator-configured allowlist).
- Bridge: `_system:service:bridge-oci`.
- Bridge run id: `brun_01HZL...`.

**Trace.**

1. L7 marketplace surface periodically refreshes its container catalogue cache; emits `bridge.import_metadata` typed action with `BridgeSource = OCI_REGISTRY`, target = "ghcr.io/curated-list".
2. **Step 1 (operation kind dispatch).** `BridgeOperationKind = METADATA_IMPORT`. OCI auto-bridge admits METADATA_IMPORT only.
3. **Step 2 (rate-limit check).** OCI metadata imports in last hour: 142. Budget: 600. Proceed.
4. **Step 3 (fetch).** Bridge fetches OCI image manifests + cosign signatures + image labels for the curated list within `EXPLICIT_ALLOWLIST` to `ghcr.io`. Each image's manifest is bounded at 16 MiB. `BRIDGE_FETCH_STARTED` and `BRIDGE_FETCH_COMPLETED` emitted per fetch (collapsed into a single per-run `BRIDGE_METADATA_IMPORTED` summary at the end).
5. **Step 4 (upstream signature verify).** `UpstreamSignatureKind = OCI_COSIGN`. Bridge runs cosign verify against the pinned Sigstore Fulcio root + per-image trust policy. Each verified image's metadata proceeds; each unverified image is dropped from the import (no `BRIDGE_UPSTREAM_SIGNATURE_FAILED` emitted per dropped image — metadata-only failures are summarised at the import-level evidence record).
6. **Step 5 (deceptive-claim check).** Each image's labels inspected; one image's `org.opencontainers.image.description` contains `"officially AIOS-verified"`. **Match.** That image is rejected with `REJECTED_DECEPTIVE`; FOREVER `BRIDGE_TRUST_CLASS_DECEPTION_DETECTED` and `BRIDGE_DECEPTIVE_REJECTED` emitted with the upstream image ref. The remaining images proceed.
7. **Step 6 (repackage).** Skipped for `METADATA_IMPORT`.
8. **Step 7 (hand-off).** Skipped for `METADATA_IMPORT`. The bridge does NOT install any package.
9. **Step 8 (admission evidence).** `BRIDGE_METADATA_IMPORTED` (STANDARD_24M) emitted with `MetadataAttribution` per imported metadata object. The L7 marketplace surface refreshes its container catalogue cache from these objects.
10. **Operator's view.** Alice browses the catalogue and sees the imported images with attribution "from GHCR". One image — the deceptive one — is not in the catalogue; FOREVER evidence remains for audit. To actually install any of these images, alice would have to invoke the explicit-fetch typed action (`bridge.oci.fetch_explicit`), which is gated by an `EXACT_ACTION` approval and runs through a separate admission pipeline (queued for `02_marketplace.md`).

**Evidence chain.** `BRIDGE_METADATA_IMPORTED` (per import) + `BRIDGE_DECEPTIVE_REJECTED` (per rejected image) + `BRIDGE_TRUST_CLASS_DECEPTION_DETECTED` (cross-referenced).

**Constitutional point.** The OCI policy is intentionally restrictive: no auto-package-import from OCI even for cosigned images. Alice's catalogue-browsing experience is rich (metadata is imported), but actual package installation requires her explicit per-package decision. The contract trades convenience for trust.

### §14.3 Example 3 — Distro repo bootstrap during first-boot

**Setup.**

- Phase: AIOS first-boot system bootstrap.
- Subject: `_system:service:bootstrap-orchestrator` (recovery-mode-equivalent system bootstrap subject).
- Bridge: `_system:service:bridge-distro-deb`.
- Bridge run id: `brun_01HZA...` (the very first bridge run on this host).
- Target packages: `linux-image-generic`, `systemd`, `init` (the substrate L1 generic-fallback kernel and basic system services).

**Trace.**

1. Bootstrap orchestrator emits `bridge.fetch` typed action with `BridgeSource = DISTRO_DEB`, targets = the bootstrap package list.
2. **Step 1 (operation kind dispatch).** `BridgeOperationKind = PACKAGE_FETCH` (followed by `PACKAGE_REPACKAGE` per package).
3. **Step 2 (rate-limit check).** First bootstrap on this host. Counter zero. Proceed.
4. **Step 3 (fetch).** Bridge fetches `Release` / `InRelease` + per-package `.deb` files from `archive.ubuntu.com` within `EXPLICIT_ALLOWLIST`. `BRIDGE_FETCH_STARTED` / `BRIDGE_FETCH_COMPLETED` per package.
5. **Step 4 (upstream signature verify).** `UpstreamSignatureKind = DISTRO_DEB_GPG`. Bridge runs `gpgv` against pinned Ubuntu archive keys. Each verified package proceeds; each unverified package aborts with `BRIDGE_UPSTREAM_SIGNATURE_FAILED` (FOREVER) and is excluded from the bootstrap (the bootstrap orchestrator decides whether the missing package is fatal).
6. **Step 5 (deceptive-claim check).** Each package's `control` file inspected; no AIOS-side trust claims (Ubuntu packages do not claim AIOS affiliation). Pass.
7. **Step 6 (repackage).** Bridge synthesises an AIOS manifest per package:
   - `package_id = pkg:bridge-distro-deb:linux-image-generic` (etc.).
   - `publisher_trust = COMMUNITY`.
   - `publisher_root_id = pub:aios-bridge`.
   - `package_signing_key_id = pks:aios-bridge:distro-deb`.
   - `originating_repository = EXTERNAL_BRIDGE`.
   - `kind = APP` (bootstrap context — these are not user-facing apps but the AIOS install pipeline still admits them as APP-tier for the `kind` schema).
   - `required_sandbox` = system-bootstrap sandbox profile (broader than user-facing bootstrap floor; specified in the bootstrap orchestrator's adapter manifest, not synthesised by this bridge).
   - `declared_capabilities` = bootstrap-derived (the bootstrap orchestrator declares system-admin-equivalent capabilities under the recovery-mode envelope; the bridge does not declare them — the bridge only synthesises the manifest skeleton, the bootstrap orchestrator augments it under recovery authority).
   - `BRIDGE_REPACKAGED_WITH_AIOS_KEY` per package (STANDARD_24M).
8. **Step 7 (hand-off).** S11.1 install pipeline runs in **recovery mode** for each package; the recovery-mode gate (S11.1 step 12) is satisfied because bootstrap is recovery-mode-equivalent (per S9.1).
9. **Step 8.** Each bootstrap package transitions to `ACTIVE`. `PACKAGE_INSTALLED` per package.
10. **Post-bootstrap.** Once bootstrap completes, the bridge's `EXPLICIT_ALLOWLIST` for distro-deb is reduced to empty (the operator's host configuration removes `archive.ubuntu.com` from the runtime allowlist). Subsequent attempts to fetch from `archive.ubuntu.com` via the distro-deb bridge fail at step 3 with `BRIDGE_DEGRADED_UPSTREAM_UNAVAILABLE` (extended-60M); the bridge effectively becomes inactive after first-boot.

**Constitutional point.** The distro-repo bridge is the substrate path: it is essential for first-boot but constitutionally limited to that. Once AIOS is up, user-facing apps come from Flathub (bridged) or the AIOS marketplace (native); distro packages are not the primary application source, only the L1 substrate.

**Evidence chain.** Per package: `BRIDGE_FETCH_STARTED` → `BRIDGE_FETCH_COMPLETED` → `BRIDGE_UPSTREAM_SIGNATURE_VERIFIED` → `BRIDGE_REPACKAGED_WITH_AIOS_KEY` → S11.1 `PACKAGE_INSTALLED` (under recovery mode). The bootstrap completes with a fully-attested chain from upstream Debian archive keys through AIOS bridge keys to the AIOS host's installed bootstrap packages.

## §15 Open deferrals

- **Snapcraft and AUR recipe imports.** S12.1 §6.5 names them; this contract does not yet implement them. Deferred to a future revision.
- **Explicit OCI fetch pipeline.** §6.2 names `bridge.oci.fetch_explicit` as the operator-explicit fetch path; the typed action and its UX live in `02_marketplace.md`. Deferred.
- **HSM-backed AIOS bridge-signing keys.** S11.1 §21 already deferred the broader HSM question; bridge-side keys inherit the deferral.
- **Cross-host bridge cache federation.** A bridge admission on one host does not propagate to peer hosts. Deferred.
- **Per-bridge byte-budget configurability.** §4.3 fixes the `outbound_byte_budget` at 4 GiB; operator configurability within bounds is queued.
- **Bridge fetch resume.** A bridge fetch that fails mid-transfer cannot resume from the partial bytes; the next attempt re-fetches from byte 0. Resumable fetches are deferred.
- **`BRIDGE_VERIFIED` trust tier.** S11.1 §21 deferred the introduction of a trust tier between `VERIFIED` and `COMMUNITY` for vetted bridges; this contract preserves that deferral.
- **Operator-extensible per-bridge allowlists.** Operators can configure additional upstream FQDNs into the per-source allowlist within bounds; the configuration UI and the bound enforcement live in `02_marketplace.md`. Deferred.
- **Distro repo as user-facing app source.** Constitutionally rejected here (§6.3); not a deferral, a rejection.

## §16 Acceptance criteria

- [ ] `BridgeSource`, `BridgeOperationKind`, `UpstreamSignatureKind`, `BridgeAdmissionResult` are closed enums with the exact value sets in §3.
- [ ] The bridge architecture (§4) anchors at `pub:aios-bridge` (AIOS_ROOT trust) with constitutional bound: bridges admit only `COMMUNITY`-trust manifests with `originating_repository = EXTERNAL_BRIDGE`.
- [ ] The bridge admission pipeline has eight ordered steps per §5; every step has a closed failure outcome.
- [ ] The per-source policy table (§6) closes which `BridgeOperationKind` values each `BridgeSource` admits: Flathub admits all four; OCI admits METADATA_IMPORT only; distro repos admit PACKAGE_FETCH for first-boot bootstrap only; OTHER_BRIDGED reserved.
- [ ] Bridges constitutionally cannot synthesise recovery-only `PackageKind` manifests (§6.5).
- [ ] Metadata import is separate from package import (§7): manifests / ratings / descriptions imported as metadata-only with mandatory source attribution; metadata never auto-grants higher trust.
- [ ] Recipe import (§8) produces `RECIPE_IMPORTED`-trust-class entries in the S12.1 community recipe registry; local Phase A and Phase C audits remain authoritative.
- [ ] Per-bridge sandbox per §4.3 with `ISOLATED_SANDBOX` floor; `LOOPBACK`-equivalent net only via `EXPLICIT_ALLOWLIST` to upstream FQDNs; `BROKER_ONLY` secrets; `GPU_NONE`.
- [ ] Per-source rate limits per §5.2 with deferral on excess and `BRIDGE_RATE_LIMIT_EXCEEDED` after `≥ 3` deferrals in 1 hour.
- [ ] Auto-blacklist conditions per §9.2; permanent deplatform on repeat per §9.3.
- [ ] All eight adversarial vectors in §10 have a named defense.
- [ ] Telemetry total cardinality ≤ 300 active label tuples per §11.
- [ ] Twelve evidence record types are queued for S3.1 per §12.
- [ ] `REJECTED_DECEPTIVE` is permanent (keyed on upstream content hash); `REJECTED_UNSIGNED` has no operator override path.
- [ ] Three worked examples (§14) trace deterministically through the pipeline.
- [ ] Cites INV-002, INV-008, INV-013, INV-014, INV-017; does not redefine S11.1 trust chain or S12.1 EcosystemRuntime.

## §17 See also

- [L10 Overview](00_overview.md)
- [S11.1 Repository Model](01_repository_model.md) — `PublisherTrustLevel`, `RepositoryKind = EXTERNAL_BRIDGE`, install pipeline, three-tier trust chain
- [S12.1 App Runtime Model](../L6_Apps_Packages_Compatibility/01_app_runtime_model.md) — `EcosystemRuntime`, `RecipeTrustClass = RECIPE_IMPORTED`, four-phase setup, community recipe registry, honesty principle
- [L0 Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md) — INV-002, INV-008, INV-013, INV-014, INV-017
- [S5.3 Approval Mechanics](../L4_Policy_Identity_Vault/04_approval_mechanics.md) — `EXACT_ACTION` binding, `HUMAN_USER` approver
- [S3.2 Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md) — `SandboxProfile`, `ISOLATED_SANDBOX`, runtime safety floor
- [S0.1 Action Envelope Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md) — typed actions, FSM
- [S3.1 Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md) — `RecordType`, retention classes
- [S8.1 Network Policy](../L8_Network_Hardware_Devices/02_network_policy.md) — `NetworkOutboundManifest`
- [S9.1 Recovery Boundary](../L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md) — recovery-mode-equivalent first-boot bootstrap
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

---

Status: `REAL`
Evidence: `E1` (file exists; structural contract complete; four closed enums declared in §3; eight-step bridge admission pipeline strictly ordered in §5; per-source policy table closes operation kinds in §6; metadata-only import discipline separate from package import in §7; recipe-only import discipline binds to S12.1 in §8; bridge reputation and auto-blacklist mechanics in §9; eight adversarial vectors with named defenses in §10; bounded-cardinality telemetry contract in §11; twelve evidence record types queued for S3.1 in §12; cross-spec dependencies enumerated in §13; three worked examples trace deterministically in §14)
